use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use crate::parser::{BinOp, Expr, Item, UnOp};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct RuntimeError {
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct EvaluationResult {
    pub env: Environment,
    pub expression_results: Vec<Value>,
}

pub type Environment = HashMap<String, Value>;

#[derive(Clone)]
pub enum Value {
    Closure(Rc<ClosureValue>),
    Nat(u64),
    Bool(bool),
}

#[derive(Clone)]
pub struct ClosureValue {
    params: Vec<String>,
    body: Expr,
    env: Environment,
    bound_args: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransportValue {
    Closure(TransportClosureValue),
    Nat(u64),
    Bool(bool),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportClosureValue {
    pub params: Vec<String>,
    pub body: Expr,
    pub env: HashMap<String, TransportValue>,
    pub bound_args: Vec<TransportValue>,
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Closure(closure) => f
                .debug_struct("Closure")
                .field("params", &closure.params)
                .field("bound_args", &closure.bound_args.len())
                .finish(),
            Value::Nat(value) => f.debug_tuple("Nat").field(value).finish(),
            Value::Bool(value) => f.debug_tuple("Bool").field(value).finish(),
        }
    }
}

pub fn interpret_program(items: &[Item]) -> Result<EvaluationResult, RuntimeError> {
    let mut env = Environment::new();
    let mut expression_results = Vec::new();

    for item in items {
        if let Some(result) = eval_item(item, &mut env)? {
            expression_results.push(result);
        }
    }

    Ok(EvaluationResult {
        env,
        expression_results,
    })
}

pub fn eval_item(item: &Item, env: &mut Environment) -> Result<Option<Value>, RuntimeError> {
    match item {
        Item::LetBinding { name, value } => {
            let evaluated = eval_expr(value, env)?;
            env.insert(name.clone(), evaluated);
            Ok(None)
        }
        Item::LetAbstraction { name, params, body } => {
            let closure = Value::Closure(Rc::new(ClosureValue {
                params: params.clone(),
                body: body.clone(),
                env: env.clone(),
                bound_args: Vec::new(),
            }));
            env.insert(name.clone(), closure);
            Ok(None)
        }
        Item::Expr(expr) => Ok(Some(eval_expr(expr, env)?)),
    }
}

pub fn eval_expr(expr: &Expr, env: &Environment) -> Result<Value, RuntimeError> {
    match expr {
        Expr::Var(name) => env.get(name).cloned().ok_or_else(|| RuntimeError {
            message: format!("Unbound variable '{name}'"),
        }),
        Expr::Nat(value) => Ok(Value::Nat(*value)),
        Expr::Bool(value) => Ok(Value::Bool(*value)),
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let condition = eval_expr(condition, env)?;
            match condition {
                Value::Bool(true) => eval_expr(then_branch, env),
                Value::Bool(false) => eval_expr(else_branch, env),
                other => Err(RuntimeError {
                    message: format!("If condition must be boolean, got {other:?}"),
                }),
            }
        }
        Expr::Binary { op, left, right } => match op {
            BinOp::And => {
                let left = eval_expr(left, env)?;
                match left {
                    Value::Bool(false) => Ok(Value::Bool(false)),
                    Value::Bool(true) => {
                        let right = eval_expr(right, env)?;
                        match right {
                                Value::Bool(value) => Ok(Value::Bool(value)),
                                other => Err(RuntimeError {
                                    message: format!(
                                        "Boolean operators require booleans, got Bool(true) and {other:?}"
                                    ),
                                }),
                            }
                    }
                    other => Err(RuntimeError {
                        message: format!(
                            "Boolean operators require booleans, got {other:?} and <rhs>"
                        ),
                    }),
                }
            }
            BinOp::Or => {
                let left = eval_expr(left, env)?;
                match left {
                    Value::Bool(true) => Ok(Value::Bool(true)),
                    Value::Bool(false) => {
                        let right = eval_expr(right, env)?;
                        match right {
                                Value::Bool(value) => Ok(Value::Bool(value)),
                                other => Err(RuntimeError {
                                    message: format!(
                                        "Boolean operators require booleans, got Bool(false) and {other:?}"
                                    ),
                                }),
                            }
                    }
                    other => Err(RuntimeError {
                        message: format!(
                            "Boolean operators require booleans, got {other:?} and <rhs>"
                        ),
                    }),
                }
            }
            _ => {
                let left = eval_expr(left, env)?;
                let right = eval_expr(right, env)?;
                eval_binary(*op, left, right)
            }
        },
        Expr::Unary { op, expr } => {
            let value = eval_expr(expr, env)?;
            eval_unary(*op, value)
        }
        Expr::App {
            function,
            arguments,
        } => {
            let mut fn_value = eval_expr(function, env)?;
            for arg in arguments {
                let arg_value = eval_expr(arg, env)?;
                fn_value = apply_one(fn_value, arg_value)?;
            }
            Ok(fn_value)
        }
    }
}

fn apply_one(function: Value, arg: Value) -> Result<Value, RuntimeError> {
    match function {
        Value::Closure(closure) => {
            if closure.params.is_empty() {
                return Err(RuntimeError {
                    message: "Cannot apply argument to zero-argument closure".to_string(),
                });
            }

            let mut new_bound_args = closure.bound_args.clone();
            new_bound_args.push(arg);

            if new_bound_args.len() < closure.params.len() {
                return Ok(Value::Closure(Rc::new(ClosureValue {
                    params: closure.params.clone(),
                    body: closure.body.clone(),
                    env: closure.env.clone(),
                    bound_args: new_bound_args,
                })));
            }

            if new_bound_args.len() > closure.params.len() {
                return Err(RuntimeError {
                    message: "Too many arguments applied to closure".to_string(),
                });
            }

            let mut call_env = closure.env.clone();
            for (param, value) in closure.params.iter().zip(new_bound_args.into_iter()) {
                call_env.insert(param.clone(), value);
            }
            eval_expr(&closure.body, &call_env)
        }
        non_function => Err(RuntimeError {
            message: format!("Cannot apply non-function value: {non_function:?}"),
        }),
    }
}

fn eval_binary(op: BinOp, left: Value, right: Value) -> Result<Value, RuntimeError> {
    let (left, right) = match (left, right) {
        (Value::Nat(left), Value::Nat(right)) => (left, right),
        (left, right) => {
            return Err(RuntimeError {
                message: format!(
                    "Numeric operators require natural numbers, got {left:?} and {right:?}"
                ),
            });
        }
    };

    match op {
        BinOp::Add => Ok(Value::Nat(left + right)),
        BinOp::Sub => {
            if left < right {
                Err(RuntimeError {
                    message: "Natural number subtraction would be negative".to_string(),
                })
            } else {
                Ok(Value::Nat(left - right))
            }
        }
        BinOp::Mul => Ok(Value::Nat(left * right)),
        BinOp::Div => {
            if right == 0 {
                Err(RuntimeError {
                    message: "Division by zero".to_string(),
                })
            } else {
                Ok(Value::Nat(left / right))
            }
        }
        BinOp::Eq => Ok(Value::Bool(left == right)),
        BinOp::NotEq => Ok(Value::Bool(left != right)),
        BinOp::Lt => Ok(Value::Bool(left < right)),
        BinOp::Le => Ok(Value::Bool(left <= right)),
        BinOp::Gt => Ok(Value::Bool(left > right)),
        BinOp::Ge => Ok(Value::Bool(left >= right)),
        BinOp::And | BinOp::Or => Err(RuntimeError {
            message: "Boolean binary op reached numeric evaluator".to_string(),
        }),
    }
}

fn eval_unary(op: UnOp, value: Value) -> Result<Value, RuntimeError> {
    match op {
        UnOp::Not => match value {
            Value::Bool(b) => Ok(Value::Bool(!b)),
            other => Err(RuntimeError {
                message: format!("'not' expects boolean operand, got {other:?}"),
            }),
        },
    }
}

impl Value {
    pub fn to_transport(&self) -> TransportValue {
        match self {
            Value::Closure(closure) => TransportValue::Closure(TransportClosureValue {
                params: closure.params.clone(),
                body: closure.body.clone(),
                env: closure
                    .env
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_transport()))
                    .collect(),
                bound_args: closure.bound_args.iter().map(Value::to_transport).collect(),
            }),
            Value::Nat(value) => TransportValue::Nat(*value),
            Value::Bool(value) => TransportValue::Bool(*value),
        }
    }

    pub fn from_transport(value: &TransportValue) -> Value {
        match value {
            TransportValue::Closure(closure) => Value::Closure(Rc::new(ClosureValue {
                params: closure.params.clone(),
                body: closure.body.clone(),
                env: closure
                    .env
                    .iter()
                    .map(|(k, v)| (k.clone(), Value::from_transport(v)))
                    .collect(),
                bound_args: closure
                    .bound_args
                    .iter()
                    .map(Value::from_transport)
                    .collect(),
            })),
            TransportValue::Nat(value) => Value::Nat(*value),
            TransportValue::Bool(value) => Value::Bool(*value),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::parser::parse_source;

    use super::{interpret_program, Value};

    #[test]
    fn evaluates_program_and_collects_top_level_expression_results() {
        let source = r#"
let id x = x
let const x y = x
let a = id
let b = const a
const b id
"#;

        let ast = parse_source(source).expect("Valid source should parse");
        let result = interpret_program(&ast).expect("Program should evaluate");

        assert_eq!(result.expression_results.len(), 1);
        match &result.expression_results[0] {
            Value::Closure(_) => {}
            other => panic!("Expected closure result, got {other:?}"),
        }
    }

    #[test]
    fn supports_partial_application() {
        let source = r#"
let const x y = x
let first = const const
first
"#;

        let ast = parse_source(source).expect("Valid source should parse");
        let result = interpret_program(&ast).expect("Program should evaluate");

        assert!(result.env.contains_key("first"));
        match result.env.get("first").expect("first must exist") {
            Value::Closure(_) => {}
            other => panic!("Expected closure binding, got {other:?}"),
        }
    }

    #[test]
    fn fails_on_unbound_variable() {
        let source = "missing";
        let ast = parse_source(source).expect("Source should parse");
        let err = interpret_program(&ast).expect_err("Unbound variable must fail");
        assert!(
            err.message.contains("Unbound variable"),
            "Unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn evaluates_if_else_and_arithmetic() {
        let source = r#"
let n = 10 + 2 * 3
if true then n / 4 else 0
"#;
        let ast = parse_source(source).expect("Source should parse");
        let result = interpret_program(&ast).expect("Program should evaluate");
        assert_eq!(result.expression_results.len(), 1);
        match &result.expression_results[0] {
            Value::Nat(value) => assert_eq!(*value, 4),
            other => panic!("Expected Nat result, got {other:?}"),
        }
    }

    #[test]
    fn evaluates_numeric_comparisons_in_if() {
        let source = r#"
if 10 - 3 * 2 == 4 then 99 else 0
"#;
        let ast = parse_source(source).expect("Source should parse");
        let result = interpret_program(&ast).expect("Program should evaluate");
        match &result.expression_results[0] {
            Value::Nat(value) => assert_eq!(*value, 99),
            other => panic!("Expected Nat result, got {other:?}"),
        }
    }

    #[test]
    fn evaluates_boolean_operators() {
        let source = r#"
if not false and (1 < 2) or false then 7 else 3
"#;
        let ast = parse_source(source).expect("Source should parse");
        let result = interpret_program(&ast).expect("Program should evaluate");
        match &result.expression_results[0] {
            Value::Nat(value) => assert_eq!(*value, 7),
            other => panic!("Expected Nat result, got {other:?}"),
        }
    }
}
