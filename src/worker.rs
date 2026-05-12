use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};

use serde::{Deserialize, Serialize};

use crate::interpreter::{eval_item, Environment, TransportValue, Value};
use crate::parser::Item;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload")]
pub enum WorkerRequest {
    Ping,
    Task(TaskRequest),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload")]
pub enum WorkerResponse {
    Pong,
    Task(TaskResponse),
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRequest {
    pub task_id: usize,
    pub item: Item,
    pub input_env: HashMap<String, TransportValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResponse {
    pub task_id: usize,
    pub produced_binding: Option<(String, TransportValue)>,
    pub expression_result: Option<TransportValue>,
    pub error: Option<String>,
}

pub fn run_worker(listener_addr: &str) -> std::io::Result<()> {
    let listener = TcpListener::bind(listener_addr)?;
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(err) = handle_connection(stream) {
                    eprintln!("Worker connection error: {err}");
                }
            }
            Err(err) => eprintln!("Worker accept error: {err}"),
        }
    }
    Ok(())
}

fn handle_connection(mut stream: TcpStream) -> Result<(), String> {
    let mut reader = BufReader::new(
        stream
            .try_clone()
            .map_err(|e| format!("Failed to clone stream: {e}"))?,
    );
    let mut line = String::new();
    let bytes = reader
        .read_line(&mut line)
        .map_err(|e| format!("Failed to read request: {e}"))?;
    if bytes == 0 {
        return Ok(());
    }

    let request: WorkerRequest =
        serde_json::from_str(&line).map_err(|e| format!("Invalid request JSON: {e}"))?;

    let response = match request {
        WorkerRequest::Ping => WorkerResponse::Pong,
        WorkerRequest::Task(request) => {
            let mut env: Environment = request
                .input_env
                .iter()
                .map(|(k, v)| (k.clone(), Value::from_transport(v)))
                .collect();

            match eval_item(&request.item, &mut env) {
                Ok(expr_result) => {
                    let produced_binding = match &request.item {
                        Item::LetBinding { name, .. } | Item::LetAbstraction { name, .. } => env
                            .get(name)
                            .map(|value| (name.clone(), value.to_transport())),
                        Item::Expr(_) => None,
                    };

                    WorkerResponse::Task(TaskResponse {
                        task_id: request.task_id,
                        produced_binding,
                        expression_result: expr_result.as_ref().map(Value::to_transport),
                        error: None,
                    })
                }
                Err(err) => WorkerResponse::Task(TaskResponse {
                    task_id: request.task_id,
                    produced_binding: None,
                    expression_result: None,
                    error: Some(err.message),
                }),
            }
        }
    };

    let payload =
        serde_json::to_string(&response).map_err(|e| format!("Failed to encode response: {e}"))?;
    stream
        .write_all(payload.as_bytes())
        .and_then(|_| stream.write_all(b"\n"))
        .map_err(|e| format!("Failed to write response: {e}"))?;
    Ok(())
}
