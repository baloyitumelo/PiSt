use std::collections::{HashMap, HashSet};

use crate::parser::{Expr, Item};

#[derive(Debug, Clone)]
pub struct DagNode {
    pub id: usize,
    pub item: Item,
    pub depends_on: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct ProgramDag {
    pub nodes: Vec<DagNode>,
}

#[derive(Debug, Clone)]
pub struct DagError {
    pub message: String,
}

pub fn build_program_dag(items: &[Item]) -> ProgramDag {
    let mut name_producers: HashMap<String, usize> = HashMap::new();
    let mut nodes = Vec::with_capacity(items.len());

    for (id, item) in items.iter().cloned().enumerate() {
        let free = free_vars_in_item(&item);
        let mut deps = HashSet::new();
        for name in free {
            if let Some(&producer) = name_producers.get(&name) {
                deps.insert(producer);
            }
        }

        if let Some(name) = item_output_name(&item) {
            name_producers.insert(name.to_string(), id);
        }

        nodes.push(DagNode {
            id,
            item,
            depends_on: deps.into_iter().collect(),
        });
    }

    ProgramDag { nodes }
}

pub fn item_output_name(item: &Item) -> Option<&str> {
    match item {
        Item::LetBinding { name, .. } => Some(name),
        Item::LetAbstraction { name, .. } => Some(name),
        Item::Expr(_) => None,
    }
}

fn free_vars_in_item(item: &Item) -> HashSet<String> {
    match item {
        Item::LetBinding { value, .. } => free_vars_expr(value),
        Item::LetAbstraction { params, body, .. } => {
            let mut set = free_vars_expr(body);
            for param in params {
                set.remove(param);
            }
            set
        }
        Item::Expr(expr) => free_vars_expr(expr),
    }
}

fn free_vars_expr(expr: &Expr) -> HashSet<String> {
    match expr {
        Expr::Var(name) => {
            let mut set = HashSet::new();
            set.insert(name.clone());
            set
        }
        Expr::Nat(_) | Expr::Bool(_) => HashSet::new(),
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let mut set = free_vars_expr(condition);
            set.extend(free_vars_expr(then_branch));
            set.extend(free_vars_expr(else_branch));
            set
        }
        Expr::Binary { left, right, .. } => {
            let mut set = free_vars_expr(left);
            set.extend(free_vars_expr(right));
            set
        }
        Expr::Unary { expr, .. } => free_vars_expr(expr),
        Expr::App {
            function,
            arguments,
        } => {
            let mut set = free_vars_expr(function);
            for arg in arguments {
                set.extend(free_vars_expr(arg));
            }
            set
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::parser::parse_source;

    use super::build_program_dag;

    #[test]
    fn builds_dependencies_from_symbol_usage() {
        let ast = parse_source(
            r#"
let id x = x
let a = id
let b = id
a b
"#,
        )
        .expect("Program should parse");

        let dag = build_program_dag(&ast);
        assert_eq!(dag.nodes.len(), 4);
        assert!(dag.nodes[1].depends_on.contains(&0));
        assert!(dag.nodes[2].depends_on.contains(&0));
        assert!(dag.nodes[3].depends_on.contains(&1));
        assert!(dag.nodes[3].depends_on.contains(&2));
    }
}
