use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::thread;
use std::time::Duration;

use crate::dag::{build_program_dag, item_output_name, ProgramDag};
use crate::interpreter::{Environment, TransportValue};
use crate::parser::Item;
use crate::worker::{TaskRequest, TaskResponse, WorkerRequest, WorkerResponse};

#[derive(Debug, Clone)]
pub struct SchedulerError {
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct ScheduleOutput {
    pub env: HashMap<String, TransportValue>,
    pub expression_results: Vec<TransportValue>,
    pub line_results: Vec<LineResult>,
}

#[derive(Debug, Clone)]
pub enum LineResult {
    Binding { name: String, value: TransportValue },
    Expression { value: TransportValue },
}

pub fn connect_to(ip: &str, port: u16) -> Result<String, SchedulerError> {
    let addr = format!("{ip}:{port}");
    let mut stream = TcpStream::connect(&addr).map_err(|e| SchedulerError {
        message: format!("Failed connecting to worker at {addr}: {e}"),
    })?;
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| SchedulerError {
            message: format!("Failed configuring read timeout for {addr}: {e}"),
        })?;
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| SchedulerError {
            message: format!("Failed configuring write timeout for {addr}: {e}"),
        })?;

    let ping = serde_json::to_string(&WorkerRequest::Ping).map_err(|e| SchedulerError {
        message: format!("Failed to encode ping request: {e}"),
    })?;
    stream
        .write_all(ping.as_bytes())
        .and_then(|_| stream.write_all(b"\n"))
        .map_err(|e| SchedulerError {
            message: format!("Failed to send ping to {addr}: {e}"),
        })?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| SchedulerError {
        message: format!("Failed to read ping response from {addr}: {e}"),
    })?;
    let response: WorkerResponse = serde_json::from_str(&line).map_err(|e| SchedulerError {
        message: format!("Failed to decode ping response from {addr}: {e}"),
    })?;

    match response {
        WorkerResponse::Pong => Ok(addr),
        WorkerResponse::Error(err) => Err(SchedulerError {
            message: format!("Worker at {addr} rejected ping: {err}"),
        }),
        WorkerResponse::Task(_) => Err(SchedulerError {
            message: format!("Worker at {addr} sent unexpected task response to ping"),
        }),
    }
}

pub fn execute_distributed(
    items: &[Item],
    worker_addrs: &[String],
) -> Result<ScheduleOutput, SchedulerError> {
    if worker_addrs.is_empty() {
        return Err(SchedulerError {
            message: "No worker addresses provided".to_string(),
        });
    }

    let dag = build_program_dag(items);
    execute_dag(&dag, worker_addrs)
}

fn execute_dag(
    dag: &ProgramDag,
    worker_addrs: &[String],
) -> Result<ScheduleOutput, SchedulerError> {
    let mut indegree: Vec<usize> = dag.nodes.iter().map(|n| n.depends_on.len()).collect();
    let mut dependents: HashMap<usize, Vec<usize>> = HashMap::new();
    for node in &dag.nodes {
        for dep in &node.depends_on {
            dependents.entry(*dep).or_default().push(node.id);
        }
    }

    let mut ready = VecDeque::new();
    for (id, degree) in indegree.iter().enumerate() {
        if *degree == 0 {
            ready.push_back(id);
        }
    }

    let mut completed = vec![false; dag.nodes.len()];
    let mut global_env: Environment = HashMap::new();
    let mut expr_results_by_node: HashMap<usize, TransportValue> = HashMap::new();
    let mut line_results_by_node: HashMap<usize, LineResult> = HashMap::new();
    let mut next_worker = 0usize;
    let mut completed_count = 0usize;

    while !ready.is_empty() {
        let mut batch = Vec::new();
        let mut handles = Vec::new();
        let max_batch = ready.len().min(worker_addrs.len());

        for _ in 0..max_batch {
            let node_id = ready.pop_front().expect("ready non-empty");
            let node = &dag.nodes[node_id];
            let addr = worker_addrs[next_worker % worker_addrs.len()].clone();
            next_worker += 1;

            let input_env: HashMap<String, TransportValue> = node
                .depends_on
                .iter()
                .filter_map(|dep_id| {
                    item_output_name(&dag.nodes[*dep_id].item).and_then(|name| {
                        global_env.get(name).map(|v| (name.to_string(), v.clone()))
                    })
                })
                .map(|(k, v)| (k, v.to_transport()))
                .collect();

            let request = TaskRequest {
                task_id: node_id,
                item: node.item.clone(),
                input_env,
            };
            batch.push(node_id);
            handles.push(thread::spawn(move || send_task(&addr, request)));
        }

        for (node_id, handle) in batch.into_iter().zip(handles.into_iter()) {
            let response = handle
                .join()
                .map_err(|_| SchedulerError {
                    message: "Worker request thread panicked".to_string(),
                })?
                .map_err(|msg| SchedulerError { message: msg })?;

            if let Some(err) = response.error {
                return Err(SchedulerError {
                    message: format!("Task {} failed: {}", response.task_id, err),
                });
            }

            if let Some((name, value)) = response.produced_binding {
                line_results_by_node.insert(
                    node_id,
                    LineResult::Binding {
                        name: name.clone(),
                        value: value.clone(),
                    },
                );
                global_env.insert(name, crate::interpreter::Value::from_transport(&value));
            }
            if let Some(value) = response.expression_result {
                line_results_by_node.insert(
                    node_id,
                    LineResult::Expression {
                        value: value.clone(),
                    },
                );
                expr_results_by_node.insert(node_id, value);
            }

            completed[node_id] = true;
            completed_count += 1;

            if let Some(children) = dependents.get(&node_id) {
                for child in children {
                    indegree[*child] -= 1;
                    if indegree[*child] == 0 {
                        ready.push_back(*child);
                    }
                }
            }
        }
    }

    if completed_count != dag.nodes.len() || completed.iter().any(|done| !done) {
        return Err(SchedulerError {
            message: "Not all tasks completed; graph may contain unresolved dependencies"
                .to_string(),
        });
    }

    let mut expression_results = Vec::new();
    let mut line_results = Vec::new();
    for node in &dag.nodes {
        if matches!(node.item, Item::Expr(_)) {
            if let Some(value) = expr_results_by_node.get(&node.id) {
                expression_results.push(value.clone());
            }
        }
        if let Some(line_result) = line_results_by_node.get(&node.id) {
            line_results.push(line_result.clone());
        }
    }

    Ok(ScheduleOutput {
        env: global_env
            .into_iter()
            .map(|(k, v)| (k, v.to_transport()))
            .collect(),
        expression_results,
        line_results,
    })
}

fn send_task(addr: &str, request: TaskRequest) -> Result<TaskResponse, String> {
    let mut stream = TcpStream::connect(addr).map_err(|e| format!("Connect {addr} failed: {e}"))?;
    let payload = serde_json::to_string(&WorkerRequest::Task(request))
        .map_err(|e| format!("Encode request failed: {e}"))?;
    stream
        .write_all(payload.as_bytes())
        .and_then(|_| stream.write_all(b"\n"))
        .map_err(|e| format!("Send request failed: {e}"))?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| format!("Read response failed: {e}"))?;
    let response: WorkerResponse =
        serde_json::from_str(&line).map_err(|e| format!("Decode response failed: {e}"))?;
    match response {
        WorkerResponse::Task(task) => Ok(task),
        WorkerResponse::Error(err) => Err(format!("Worker error: {err}")),
        WorkerResponse::Pong => Err("Unexpected pong response to task".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::thread;
    use std::time::Duration;

    use crate::parser::parse_source;
    use crate::worker::run_worker;

    use super::{connect_to, execute_distributed};

    #[test]
    fn executes_program_across_multiple_workers() {
        let worker1 = "127.0.0.1:43101".to_string();
        let worker2 = "127.0.0.1:43102".to_string();

        let addr1 = worker1.clone();
        let addr2 = worker2.clone();
        thread::spawn(move || {
            let _ = run_worker(&addr1);
        });
        thread::spawn(move || {
            let _ = run_worker(&addr2);
        });
        thread::sleep(Duration::from_millis(80));

        let source = r#"
let id x = x
let a = id
let b = id
a b
"#;
        let ast = parse_source(source).expect("Program should parse");
        let out = execute_distributed(&ast, &[worker1, worker2]).expect("Scheduling should work");

        assert!(out.env.contains_key("a"));
        assert!(out.env.contains_key("b"));
        assert_eq!(out.expression_results.len(), 1);
    }

    #[test]
    fn connect_to_checks_existing_worker() {
        let worker = "127.0.0.1:43103".to_string();
        let addr = worker.clone();
        thread::spawn(move || {
            let _ = run_worker(&addr);
        });
        thread::sleep(Duration::from_millis(80));

        let connected = connect_to("127.0.0.1", 43103).expect("Should connect to worker");
        assert_eq!(connected, worker);
    }
}
