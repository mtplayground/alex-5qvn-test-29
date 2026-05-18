use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value as JsonValue};
use sqlx::PgPool;
use uuid::Uuid;

use crate::ast::{ComparisonOp, Expr, Literal, PropertyAccess, RelationshipDirection};
use crate::domain::{Edge, Node, Properties};
use crate::planner::{
    CreateEdge, CreateNode, EdgeExpand, LogicalPlan, MergeNode, PlanStep, Projection,
    ProjectionItem,
};
use crate::repository::{EdgeRepository, NodeRepository};

const DEFAULT_SCAN_LIMIT: i64 = 1_000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Node(Node),
    Edge(Edge),
    Scalar(JsonValue),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExecutorError {
    pub message: String,
}

impl ExecutorError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Executor {
    nodes: NodeRepository,
    edges: EdgeRepository,
}

pub async fn execute_plan(pool: PgPool, plan: &LogicalPlan) -> Result<ExecutionResult, ExecutorError> {
    Executor::new(pool).execute(plan).await
}

impl Executor {
    pub fn new(pool: PgPool) -> Self {
        Self {
            nodes: NodeRepository::new(pool.clone()),
            edges: EdgeRepository::new(pool),
        }
    }

    pub async fn execute(&self, plan: &LogicalPlan) -> Result<ExecutionResult, ExecutorError> {
        let mut rows: Vec<BindingRow> = Vec::new();
        let mut projected = None;

        for step in &plan.steps {
            match step {
                PlanStep::NodeScan(scan) => {
                    rows = self.execute_node_scan(scan, rows).await?;
                    projected = None;
                }
                PlanStep::CreateNode(create) => {
                    rows = self.execute_create_node(create, rows).await?;
                    projected = None;
                }
                PlanStep::CreateEdge(create) => {
                    rows = self.execute_create_edge(create, rows).await?;
                    projected = None;
                }
                PlanStep::MergeNode(merge) => {
                    rows = self.execute_merge_node(merge, rows).await?;
                    projected = None;
                }
                PlanStep::Expand(expand) => {
                    rows = self.execute_expand(expand, rows).await?;
                    projected = None;
                }
                PlanStep::Filter(expr) => {
                    rows = self.execute_filter(expr, rows)?;
                    projected = None;
                }
                PlanStep::Project(projection) => {
                    projected = Some(self.execute_project(projection, &rows)?);
                }
                PlanStep::Limit(limit) => {
                    if let Some(result) = projected.as_mut() {
                        result.rows.truncate((*limit).try_into().unwrap_or(usize::MAX));
                    } else {
                        rows.truncate((*limit).try_into().unwrap_or(usize::MAX));
                    }
                }
            }
        }

        match projected {
            Some(result) => Ok(result),
            None => Ok(materialize_rows(&rows)),
        }
    }

    async fn execute_node_scan(
        &self,
        scan: &crate::planner::NodeScan,
        input: Vec<BindingRow>,
    ) -> Result<Vec<BindingRow>, ExecutorError> {
        let candidates = self.load_node_scan_candidates(scan).await?;
        let base_rows = if input.is_empty() {
            vec![BindingRow::new()]
        } else {
            input
        };

        let mut output = Vec::new();
        for row in base_rows {
            for node in &candidates {
                if let Some(existing) = row.get(&scan.binding) {
                    if existing != &Value::Node(node.clone()) {
                        continue;
                    }
                    output.push(row.clone());
                    continue;
                }

                let mut next = row.clone();
                next.insert(scan.binding.clone(), Value::Node(node.clone()));
                output.push(next);
            }
        }

        Ok(output)
    }

    async fn load_node_scan_candidates(
        &self,
        scan: &crate::planner::NodeScan,
    ) -> Result<Vec<Node>, ExecutorError> {
        let mut candidates = if let Some(label) = scan.labels.first() {
            self.nodes
                .scan_by_label(label, DEFAULT_SCAN_LIMIT)
                .await
                .map_err(repository_error)?
        } else {
            self.nodes
                .scan(DEFAULT_SCAN_LIMIT)
                .await
                .map_err(repository_error)?
        };

        candidates.retain(|node| node_matches_scan(node, scan));
        Ok(candidates)
    }

    async fn execute_create_node(
        &self,
        create: &CreateNode,
        input: Vec<BindingRow>,
    ) -> Result<Vec<BindingRow>, ExecutorError> {
        let base_rows = if input.is_empty() {
            vec![BindingRow::new()]
        } else {
            input
        };

        let mut output = Vec::with_capacity(base_rows.len());

        for row in base_rows {
            let node = Node {
                id: Uuid::new_v4(),
                labels: create.labels.clone(),
                properties: literals_to_properties(&create.properties)?,
            };
            let inserted = self
                .nodes
                .insert(&node)
                .await
                .map_err(repository_error)?;

            let mut next = row;
            next.insert(create.binding.clone(), Value::Node(inserted));
            output.push(next);
        }

        Ok(output)
    }

    async fn execute_create_edge(
        &self,
        create: &CreateEdge,
        input: Vec<BindingRow>,
    ) -> Result<Vec<BindingRow>, ExecutorError> {
        let mut output = Vec::with_capacity(input.len());

        for row in input {
            let from_node = require_node_binding(&row, &create.from_binding)?;
            let to_node = require_node_binding(&row, &create.to_binding)?;

            let (start_id, end_id) = create_edge_endpoints(
                &create.direction,
                from_node.id,
                to_node.id,
            );

            let edge = Edge {
                id: Uuid::new_v4(),
                start_id,
                end_id,
                type_: create.edge_type.clone(),
                properties: literals_to_properties(&create.edge_properties)?,
            };
            let inserted = self
                .edges
                .insert(&edge)
                .await
                .map_err(repository_error)?;

            let mut next = row;
            next.insert(create.edge_binding.clone(), Value::Edge(inserted));
            output.push(next);
        }

        Ok(output)
    }

    async fn execute_merge_node(
        &self,
        merge: &MergeNode,
        input: Vec<BindingRow>,
    ) -> Result<Vec<BindingRow>, ExecutorError> {
        let base_rows = if input.is_empty() {
            vec![BindingRow::new()]
        } else {
            input
        };

        let mut output = Vec::with_capacity(base_rows.len());
        let match_value = literal_to_json(&merge.value);

        for row in base_rows {
            let node = match self
                .nodes
                .get_by_label_and_property(&merge.label, &merge.key, match_value.clone())
                .await
                .map_err(repository_error)?
            {
                Some(existing) => existing,
                None => {
                    let node = Node {
                        id: Uuid::new_v4(),
                        labels: vec![merge.label.clone()],
                        properties: literals_to_properties(&merge.properties)?,
                    };
                    self.nodes.insert(&node).await.map_err(repository_error)?
                }
            };

            let mut next = row;
            next.insert(merge.binding.clone(), Value::Node(node));
            output.push(next);
        }

        Ok(output)
    }

    async fn execute_expand(
        &self,
        expand: &EdgeExpand,
        input: Vec<BindingRow>,
    ) -> Result<Vec<BindingRow>, ExecutorError> {
        let mut output = Vec::new();

        for row in input {
            let from_node = match row.get(&expand.from_binding) {
                Some(Value::Node(node)) => node,
                Some(_) => {
                    return Err(ExecutorError::new(format!(
                        "binding '{}' is not a node",
                        expand.from_binding
                    )));
                }
                None => {
                    return Err(ExecutorError::new(format!(
                        "missing node binding '{}'",
                        expand.from_binding
                    )));
                }
            };

            let expansion = self
                .edges
                .expand_neighbors(from_node.id, expand.edge_type.as_deref())
                .await
                .map_err(repository_error)?;

            let neighbor_lookup = expansion
                .nodes
                .iter()
                .map(|node| (node.id, node.clone()))
                .collect::<HashMap<_, _>>();

            for edge in &expansion.edges {
                let adjacent_id = match edge_direction_matches(edge, from_node.id, &expand.direction)
                {
                    Some(adjacent_id) => adjacent_id,
                    None => continue,
                };

                if !properties_match(&edge.properties, &expand.edge_properties) {
                    continue;
                }

                let Some(adjacent_node) = neighbor_lookup.get(&adjacent_id) else {
                    continue;
                };

                if !node_matches_expand_target(adjacent_node, expand) {
                    continue;
                }

                if let Some(existing) = row.get(&expand.edge_binding) {
                    if existing != &Value::Edge(edge.clone()) {
                        continue;
                    }
                }

                if let Some(existing) = row.get(&expand.to_binding) {
                    if existing != &Value::Node(adjacent_node.clone()) {
                        continue;
                    }
                }

                let mut next = row.clone();
                next.insert(expand.edge_binding.clone(), Value::Edge(edge.clone()));
                next.insert(expand.to_binding.clone(), Value::Node(adjacent_node.clone()));
                output.push(next);
            }
        }

        Ok(output)
    }

    fn execute_filter(
        &self,
        expr: &Expr,
        input: Vec<BindingRow>,
    ) -> Result<Vec<BindingRow>, ExecutorError> {
        let mut output = Vec::new();

        for row in input {
            if truthy(&evaluate_expr(expr, &row)?)? {
                output.push(row);
            }
        }

        Ok(output)
    }

    fn execute_project(
        &self,
        projection: &Projection,
        input: &[BindingRow],
    ) -> Result<ExecutionResult, ExecutorError> {
        let columns = projection_columns(projection, input);
        let mut rows = Vec::with_capacity(input.len());

        for row in input {
            let mut tuple = Vec::new();

            for item in &projection.items {
                match item {
                    ProjectionItem::All => {
                        for binding in row.keys() {
                            if let Some(value) = row.get(binding) {
                                tuple.push(value.clone());
                            }
                        }
                    }
                    ProjectionItem::Identifier(identifier) => {
                        let value = row.get(identifier).ok_or_else(|| {
                            ExecutorError::new(format!("missing projection binding '{identifier}'"))
                        })?;
                        tuple.push(value.clone());
                    }
                    ProjectionItem::PropertyAccess(access) => {
                        tuple.push(Value::Scalar(resolve_property_access(access, row)?));
                    }
                }
            }

            rows.push(tuple);
        }

        Ok(ExecutionResult { columns, rows })
    }
}

type BindingRow = BTreeMap<String, Value>;

fn projection_columns(projection: &Projection, rows: &[BindingRow]) -> Vec<String> {
    let mut columns = Vec::new();

    for item in &projection.items {
        match item {
            ProjectionItem::All => {
                if let Some(row) = rows.first() {
                    columns.extend(row.keys().cloned());
                }
            }
            ProjectionItem::Identifier(identifier) => columns.push(identifier.clone()),
            ProjectionItem::PropertyAccess(access) => columns.push(format!(
                "{}.{}",
                access.root,
                access.fields.join(".")
            )),
        }
    }

    columns
}

fn materialize_rows(rows: &[BindingRow]) -> ExecutionResult {
    let columns: Vec<String> = rows
        .first()
        .map(|row| row.keys().cloned().collect())
        .unwrap_or_default();

    let tuples = rows
        .iter()
        .map(|row| {
            columns
                .iter()
                .filter_map(|column| row.get(column).cloned())
                .collect()
        })
        .collect();

    ExecutionResult {
        columns,
        rows: tuples,
    }
}

fn require_node_binding<'a>(row: &'a BindingRow, binding: &str) -> Result<&'a Node, ExecutorError> {
    match row.get(binding) {
        Some(Value::Node(node)) => Ok(node),
        Some(_) => Err(ExecutorError::new(format!(
            "binding '{binding}' is not a node"
        ))),
        None => Err(ExecutorError::new(format!(
            "missing node binding '{binding}'"
        ))),
    }
}

fn create_edge_endpoints(
    direction: &RelationshipDirection,
    from_id: Uuid,
    to_id: Uuid,
) -> (Uuid, Uuid) {
    match direction {
        RelationshipDirection::Left => (to_id, from_id),
        RelationshipDirection::Right | RelationshipDirection::Undirected => (from_id, to_id),
    }
}

fn node_matches_scan(node: &Node, scan: &crate::planner::NodeScan) -> bool {
    labels_match(&node.labels, &scan.labels) && properties_match(&node.properties, &scan.properties)
}

fn node_matches_expand_target(node: &Node, expand: &EdgeExpand) -> bool {
    labels_match(&node.labels, &expand.to_labels)
        && properties_match(&node.properties, &expand.to_properties)
}

fn labels_match(actual: &[String], expected: &[String]) -> bool {
    expected.iter().all(|label| actual.iter().any(|item| item == label))
}

fn properties_match(actual: &Properties, expected: &BTreeMap<String, Literal>) -> bool {
    expected.iter().all(|(key, literal)| {
        actual
            .get(key)
            .map(|value| value == &literal_to_json(literal))
            .unwrap_or(false)
    })
}

fn edge_direction_matches(
    edge: &Edge,
    from_id: uuid::Uuid,
    direction: &RelationshipDirection,
) -> Option<uuid::Uuid> {
    match direction {
        RelationshipDirection::Right if edge.start_id == from_id => Some(edge.end_id),
        RelationshipDirection::Left if edge.end_id == from_id => Some(edge.start_id),
        RelationshipDirection::Undirected if edge.start_id == from_id => Some(edge.end_id),
        RelationshipDirection::Undirected if edge.end_id == from_id => Some(edge.start_id),
        _ => None,
    }
}

fn evaluate_expr(expr: &Expr, row: &BindingRow) -> Result<Value, ExecutorError> {
    match expr {
        Expr::Identifier(identifier) => row
            .get(identifier)
            .cloned()
            .ok_or_else(|| ExecutorError::new(format!("missing binding '{identifier}'"))),
        Expr::PropertyAccess(access) => Ok(Value::Scalar(resolve_property_access(access, row)?)),
        Expr::Literal(literal) => Ok(Value::Scalar(literal_to_json(literal))),
        Expr::Comparison { left, op, right } => {
            let left = value_to_json(evaluate_expr(left, row)?);
            let right = value_to_json(evaluate_expr(right, row)?);
            Ok(Value::Scalar(JsonValue::Bool(compare_values(op, &left, &right)?)))
        }
        Expr::And(parts) => {
            for part in parts {
                if !truthy(&evaluate_expr(part, row)?)? {
                    return Ok(Value::Scalar(JsonValue::Bool(false)));
                }
            }
            Ok(Value::Scalar(JsonValue::Bool(true)))
        }
        Expr::Or(parts) => {
            for part in parts {
                if truthy(&evaluate_expr(part, row)?)? {
                    return Ok(Value::Scalar(JsonValue::Bool(true)));
                }
            }
            Ok(Value::Scalar(JsonValue::Bool(false)))
        }
        Expr::Not(inner) => Ok(Value::Scalar(JsonValue::Bool(!truthy(
            &evaluate_expr(inner, row)?,
        )?))),
    }
}

fn compare_values(
    op: &ComparisonOp,
    left: &JsonValue,
    right: &JsonValue,
) -> Result<bool, ExecutorError> {
    Ok(match op {
        ComparisonOp::Eq => left == right,
        ComparisonOp::NotEq => left != right,
        ComparisonOp::Gt => compare_ordering(left, right, |lhs, rhs| lhs > rhs)?,
        ComparisonOp::Gte => compare_ordering(left, right, |lhs, rhs| lhs >= rhs)?,
        ComparisonOp::Lt => compare_ordering(left, right, |lhs, rhs| lhs < rhs)?,
        ComparisonOp::Lte => compare_ordering(left, right, |lhs, rhs| lhs <= rhs)?,
    })
}

fn compare_ordering(
    left: &JsonValue,
    right: &JsonValue,
    predicate: impl FnOnce(ComparableValue<'_>, ComparableValue<'_>) -> bool,
) -> Result<bool, ExecutorError> {
    let left = comparable_value(left)?;
    let right = comparable_value(right)?;

    if left.kind() != right.kind() {
        return Err(ExecutorError::new(
            "cannot compare values of different kinds",
        ));
    }

    Ok(predicate(left, right))
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum ComparableValue<'a> {
    Number(f64),
    String(&'a str),
    Bool(bool),
}

impl<'a> ComparableValue<'a> {
    fn kind(&self) -> &'static str {
        match self {
            ComparableValue::Number(_) => "number",
            ComparableValue::String(_) => "string",
            ComparableValue::Bool(_) => "bool",
        }
    }
}

impl<'a> PartialOrd for ComparableValue<'a> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (self, other) {
            (ComparableValue::Number(lhs), ComparableValue::Number(rhs)) => lhs.partial_cmp(rhs),
            (ComparableValue::String(lhs), ComparableValue::String(rhs)) => lhs.partial_cmp(rhs),
            (ComparableValue::Bool(lhs), ComparableValue::Bool(rhs)) => lhs.partial_cmp(rhs),
            _ => None,
        }
    }
}

fn comparable_value(value: &JsonValue) -> Result<ComparableValue<'_>, ExecutorError> {
    match value {
        JsonValue::Number(number) => number
            .as_f64()
            .map(ComparableValue::Number)
            .ok_or_else(|| ExecutorError::new("numeric value is out of supported range")),
        JsonValue::String(text) => Ok(ComparableValue::String(text)),
        JsonValue::Bool(flag) => Ok(ComparableValue::Bool(*flag)),
        _ => Err(ExecutorError::new("value is not orderable")),
    }
}

fn truthy(value: &Value) -> Result<bool, ExecutorError> {
    match value {
        Value::Scalar(JsonValue::Bool(flag)) => Ok(*flag),
        Value::Scalar(other) => Err(ExecutorError::new(format!(
            "expected boolean expression result, got {other}"
        ))),
        Value::Node(_) | Value::Edge(_) => Err(ExecutorError::new(
            "expected boolean expression result, got graph value",
        )),
    }
}

fn resolve_property_access(
    access: &PropertyAccess,
    row: &BindingRow,
) -> Result<JsonValue, ExecutorError> {
    let value = row
        .get(&access.root)
        .ok_or_else(|| ExecutorError::new(format!("missing binding '{}'", access.root)))?;

    resolve_property_path(value, &access.fields)
}

fn resolve_property_path(value: &Value, fields: &[String]) -> Result<JsonValue, ExecutorError> {
    match value {
        Value::Node(node) => resolve_graph_properties(node_to_json(node), &node.properties, fields),
        Value::Edge(edge) => resolve_graph_properties(edge_to_json(edge), &edge.properties, fields),
        Value::Scalar(json) => resolve_json_path(json.clone(), fields),
    }
}

fn resolve_graph_properties(
    full_value: JsonValue,
    properties: &Properties,
    fields: &[String],
) -> Result<JsonValue, ExecutorError> {
    let Some((first, rest)) = fields.split_first() else {
        return Ok(full_value);
    };

    if let Some(value) = properties.get(first) {
        return resolve_json_path(value.clone(), rest);
    }

    match &full_value {
        JsonValue::Object(map) => map.get(first).cloned().map_or_else(
            || {
                Err(ExecutorError::new(format!(
                    "property '{}' not found",
                    first
                )))
            },
            |value| resolve_json_path(value, rest),
        ),
        _ => Err(ExecutorError::new("graph value is not addressable")),
    }
}

fn resolve_json_path(mut current: JsonValue, fields: &[String]) -> Result<JsonValue, ExecutorError> {
    for field in fields {
        current = match current {
            JsonValue::Object(map) => map.get(field).cloned().ok_or_else(|| {
                ExecutorError::new(format!("property '{}' not found", field))
            })?,
            _ => {
                return Err(ExecutorError::new(format!(
                    "cannot descend into property '{}'",
                    field
                )))
            }
        };
    }

    Ok(current)
}

fn value_to_json(value: Value) -> JsonValue {
    match value {
        Value::Node(node) => node_to_json(&node),
        Value::Edge(edge) => edge_to_json(&edge),
        Value::Scalar(value) => value,
    }
}

fn node_to_json(node: &Node) -> JsonValue {
    let mut map = Map::new();
    map.insert("id".to_owned(), JsonValue::String(node.id.to_string()));
    map.insert(
        "labels".to_owned(),
        JsonValue::Array(
            node.labels
                .iter()
                .cloned()
                .map(JsonValue::String)
                .collect(),
        ),
    );
    map.insert("properties".to_owned(), JsonValue::Object(node.properties.clone()));
    JsonValue::Object(map)
}

fn edge_to_json(edge: &Edge) -> JsonValue {
    let mut map = Map::new();
    map.insert("id".to_owned(), JsonValue::String(edge.id.to_string()));
    map.insert(
        "start_id".to_owned(),
        JsonValue::String(edge.start_id.to_string()),
    );
    map.insert(
        "end_id".to_owned(),
        JsonValue::String(edge.end_id.to_string()),
    );
    map.insert("type".to_owned(), JsonValue::String(edge.type_.clone()));
    map.insert("properties".to_owned(), JsonValue::Object(edge.properties.clone()));
    JsonValue::Object(map)
}

fn literals_to_properties(
    entries: &BTreeMap<String, Literal>,
) -> Result<Properties, ExecutorError> {
    let mut properties = Map::new();

    for (key, value) in entries {
        properties.insert(key.clone(), literal_to_json_checked(value)?);
    }

    Ok(properties)
}

fn literal_to_json(literal: &Literal) -> JsonValue {
    match literal {
        Literal::Null => JsonValue::Null,
        Literal::Boolean(flag) => JsonValue::Bool(*flag),
        Literal::Integer(value) => JsonValue::Number(Number::from(*value)),
        Literal::Float(value) => Number::from_f64(*value)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null),
        Literal::String(text) => JsonValue::String(text.clone()),
        Literal::List(items) => JsonValue::Array(items.iter().map(literal_to_json).collect()),
        Literal::Map(entries) => JsonValue::Object(
            entries
                .iter()
                .map(|(key, value)| (key.clone(), literal_to_json(value)))
                .collect(),
        ),
    }
}

fn literal_to_json_checked(literal: &Literal) -> Result<JsonValue, ExecutorError> {
    match literal {
        Literal::Float(value) => Number::from_f64(*value)
            .map(JsonValue::Number)
            .ok_or_else(|| ExecutorError::new("float literal is not representable as JSON")),
        Literal::Map(entries) => {
            let mut object = Map::new();
            for (key, value) in entries {
                object.insert(key.clone(), literal_to_json_checked(value)?);
            }
            Ok(JsonValue::Object(object))
        }
        Literal::List(items) => {
            let mut list = Vec::with_capacity(items.len());
            for item in items {
                list.push(literal_to_json_checked(item)?);
            }
            Ok(JsonValue::Array(list))
        }
        Literal::Null
        | Literal::Boolean(_)
        | Literal::Integer(_)
        | Literal::String(_) => Ok(literal_to_json(literal)),
    }
}

fn repository_error(error: sqlx::Error) -> ExecutorError {
    ExecutorError::new(format!("repository error: {error}"))
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde_json::{json, Value as JsonValue};
    use sqlx::postgres::PgPoolOptions;
    use uuid::Uuid;

    use crate::ast::RelationshipDirection;
    use crate::parser::parse_ast;
    use crate::planner::plan_query;
    use crate::MIGRATOR;

    use super::{
        create_edge_endpoints, execute_plan, resolve_json_path, Executor, ExecutionResult, Value,
    };
    use crate::domain::{Edge, Node, Properties};
    use crate::repository::{EdgeRepository, NodeRepository};

    #[test]
    fn resolves_json_paths_for_projection_scalars() {
        let value = json!({
            "name": {
                "first": "Alice"
            }
        });

        let resolved = resolve_json_path(
            value,
            &["name".to_owned(), "first".to_owned()],
        )
        .expect("path should resolve");

        assert_eq!(resolved, JsonValue::String("Alice".to_owned()));
    }

    #[test]
    fn create_edge_endpoints_follow_direction() {
        let left = create_edge_endpoints(
            &RelationshipDirection::Left,
            Uuid::from_u128(1),
            Uuid::from_u128(2),
        );
        let right = create_edge_endpoints(
            &RelationshipDirection::Right,
            Uuid::from_u128(1),
            Uuid::from_u128(2),
        );

        assert_eq!(left, (Uuid::from_u128(2), Uuid::from_u128(1)));
        assert_eq!(right, (Uuid::from_u128(1), Uuid::from_u128(2)));
    }

    #[tokio::test]
    async fn executes_zero_hop_scan_project_and_limit() -> Result<(), sqlx::Error> {
        let Some(pool) = test_pool().await? else {
            return Ok(());
        };
        let fixture = fixture("zero-hop");
        let node_repository = NodeRepository::new(pool.clone());
        let alice = sample_node(
            Uuid::from_u128(fixture.base + 1),
            vec!["Person"],
            json!({"name": fixture.alice.clone(),"age":31}),
        );
        let bob = sample_node(
            Uuid::from_u128(fixture.base + 2),
            vec!["Person"],
            json!({"name": fixture.bob.clone(),"age":25}),
        );
        node_repository.insert(&alice).await?;
        node_repository.insert(&bob).await?;

        let plan = plan_query(
            &parse_ast(&format!(
                r#"MATCH (n:Person {{name: "{name}"}}) RETURN n LIMIT 1"#,
                name = fixture.alice
            ))
            .expect("query should parse"),
        )
        .expect("query should plan");

        let result = execute_plan(pool, &plan).await.expect("plan should execute");

        assert_eq!(
            result,
            ExecutionResult {
                columns: vec!["n".to_owned()],
                rows: vec![vec![Value::Node(alice)]],
            }
        );

        Ok(())
    }

    #[tokio::test]
    async fn executes_one_hop_expand_filter_and_project() -> Result<(), sqlx::Error> {
        let Some(pool) = test_pool().await? else {
            return Ok(());
        };
        let fixture = fixture("one-hop");
        seed_one_hop_graph(&pool, &fixture).await?;

        let plan = plan_query(
            &parse_ast(&format!(
                r#"
                MATCH (n:Person {{name: "{name}"}})-[r:KNOWS]->(m:Person)
                WHERE m.age >= 30
                RETURN n, r, m.name
                LIMIT 10
                "#,
                name = fixture.alice
            ))
            .expect("query should parse"),
        )
        .expect("query should plan");

        let result = Executor::new(pool.clone())
            .execute(&plan)
            .await
            .expect("plan should execute");

        assert_eq!(result.columns, vec!["n".to_owned(), "r".to_owned(), "m.name".to_owned()]);
        assert_eq!(result.rows.len(), 1);
        assert!(matches!(&result.rows[0][0], Value::Node(node) if node.properties.get("name") == Some(&JsonValue::String("Alice".to_owned()))));
        assert!(matches!(&result.rows[0][1], Value::Edge(edge) if edge.type_ == "KNOWS"));
        assert_eq!(result.rows[0][2], Value::Scalar(JsonValue::String("Bob".to_owned())));

        Ok(())
    }

    #[tokio::test]
    async fn executes_two_hop_expand_chain() -> Result<(), sqlx::Error> {
        let Some(pool) = test_pool().await? else {
            return Ok(());
        };
        let fixture = fixture("two-hop");
        seed_two_hop_graph(&pool, &fixture).await?;

        let plan = plan_query(
            &parse_ast(&format!(
                r#"
                MATCH (:Person {{name: "{name}"}})-[:KNOWS]->(friend:Person)<-[:WORKS_WITH]-(coworker:Person)
                RETURN friend, coworker
                "#,
                name = fixture.alice
            ))
            .expect("query should parse"),
        )
        .expect("query should plan");

        let result = Executor::new(pool)
            .execute(&plan)
            .await
            .expect("plan should execute");

        assert_eq!(result.columns, vec!["friend".to_owned(), "coworker".to_owned()]);
        assert_eq!(result.rows.len(), 1);
        assert!(matches!(&result.rows[0][0], Value::Node(node) if node.properties.get("name") == Some(&JsonValue::String("Bob".to_owned()))));
        assert!(matches!(&result.rows[0][1], Value::Node(node) if node.properties.get("name") == Some(&JsonValue::String("Carol".to_owned()))));

        Ok(())
    }

    #[tokio::test]
    async fn executes_create_node_and_returns_inserted_value() -> Result<(), sqlx::Error> {
        let Some(pool) = test_pool().await? else {
            return Ok(());
        };
        let fixture = fixture("create-node");

        let plan = plan_query(
            &parse_ast(&format!(
                r#"CREATE (n:Person {{name: "{name}", age: 31}}) RETURN n"#,
                name = fixture.alice
            ))
            .expect("query should parse"),
        )
        .expect("query should plan");

        let result = Executor::new(pool.clone())
            .execute(&plan)
            .await
            .expect("plan should execute");

        assert_eq!(result.columns, vec!["n".to_owned()]);
        assert_eq!(result.rows.len(), 1);
        assert!(matches!(
            &result.rows[0][0],
            Value::Node(node)
                if node.labels == vec!["Person".to_owned()]
                    && node.properties.get("name") == Some(&JsonValue::String(fixture.alice.clone()))
                    && node.properties.get("age") == Some(&JsonValue::Number(31.into()))
        ));

        Ok(())
    }

    #[tokio::test]
    async fn executes_create_edge_pattern_and_returns_created_bindings() -> Result<(), sqlx::Error> {
        let Some(pool) = test_pool().await? else {
            return Ok(());
        };
        let fixture = fixture("create-edge");

        let plan = plan_query(
            &parse_ast(&format!(
                r#"
                CREATE (a:Person {{name: "{alice}"}})-[r:KNOWS {{since: 2024}}]->(b:Person {{name: "{bob}"}})
                RETURN a, r, b
                "#,
                alice = fixture.alice,
                bob = fixture.bob
            ))
            .expect("query should parse"),
        )
        .expect("query should plan");

        let result = Executor::new(pool.clone())
            .execute(&plan)
            .await
            .expect("plan should execute");

        assert_eq!(result.columns, vec!["a".to_owned(), "r".to_owned(), "b".to_owned()]);
        assert_eq!(result.rows.len(), 1);
        assert!(matches!(
            &result.rows[0][0],
            Value::Node(node)
                if node.properties.get("name") == Some(&JsonValue::String(fixture.alice.clone()))
        ));
        assert!(matches!(
            &result.rows[0][1],
            Value::Edge(edge)
                if edge.type_ == "KNOWS"
                    && edge.properties.get("since") == Some(&JsonValue::Number(2024.into()))
        ));
        assert!(matches!(
            &result.rows[0][2],
            Value::Node(node)
                if node.properties.get("name") == Some(&JsonValue::String(fixture.bob.clone()))
        ));

        Ok(())
    }

    #[tokio::test]
    async fn executes_merge_node_insert_and_match_paths() -> Result<(), sqlx::Error> {
        let Some(pool) = test_pool().await? else {
            return Ok(());
        };
        let fixture = fixture("merge");

        let query = format!(
            r#"MERGE (n:Person {{email: "{email}"}}) RETURN n"#,
            email = fixture.alice
        );
        let plan = plan_query(&parse_ast(&query).expect("query should parse"))
            .expect("query should plan");

        let first = Executor::new(pool.clone())
            .execute(&plan)
            .await
            .expect("first merge should execute");
        let second = Executor::new(pool.clone())
            .execute(&plan)
            .await
            .expect("second merge should execute");

        assert_eq!(first.columns, vec!["n".to_owned()]);
        assert_eq!(second.columns, vec!["n".to_owned()]);
        assert_eq!(first.rows.len(), 1);
        assert_eq!(second.rows.len(), 1);

        let first_node = match &first.rows[0][0] {
            Value::Node(node) => node,
            other => panic!("expected node, got {other:?}"),
        };
        let second_node = match &second.rows[0][0] {
            Value::Node(node) => node,
            other => panic!("expected node, got {other:?}"),
        };

        assert_eq!(first_node.id, second_node.id);
        assert_eq!(
            first_node.properties.get("email"),
            Some(&JsonValue::String(fixture.alice))
        );

        Ok(())
    }

    fn sample_node(id: Uuid, labels: Vec<&str>, properties: JsonValue) -> Node {
        Node {
            id,
            labels: labels.into_iter().map(str::to_owned).collect(),
            properties: as_properties(properties),
        }
    }

    fn sample_edge(id: Uuid, start_id: Uuid, end_id: Uuid, edge_type: &str, properties: JsonValue) -> Edge {
        Edge {
            id,
            start_id,
            end_id,
            type_: edge_type.to_owned(),
            properties: as_properties(properties),
        }
    }

    fn as_properties(value: JsonValue) -> Properties {
        match value {
            JsonValue::Object(map) => map,
            _ => panic!("expected JSON object"),
        }
    }

    async fn seed_one_hop_graph(pool: &sqlx::PgPool, fixture: &Fixture) -> Result<(), sqlx::Error> {
        let node_repository = NodeRepository::new(pool.clone());
        let edge_repository = EdgeRepository::new(pool.clone());

        let alice = sample_node(
            Uuid::from_u128(fixture.base + 1),
            vec!["Person"],
            json!({"name": fixture.alice.clone(),"age":31}),
        );
        let bob = sample_node(
            Uuid::from_u128(fixture.base + 2),
            vec!["Person"],
            json!({"name": fixture.bob.clone(),"age":30}),
        );
        let carol = sample_node(
            Uuid::from_u128(fixture.base + 3),
            vec!["Person"],
            json!({"name": fixture.carol.clone(),"age":22}),
        );

        node_repository.insert(&alice).await?;
        node_repository.insert(&bob).await?;
        node_repository.insert(&carol).await?;

        edge_repository
            .insert(&sample_edge(
                Uuid::from_u128(fixture.base + 11),
                alice.id,
                bob.id,
                "KNOWS",
                json!({"since": 2020}),
            ))
            .await?;
        edge_repository
            .insert(&sample_edge(
                Uuid::from_u128(fixture.base + 12),
                alice.id,
                carol.id,
                "KNOWS",
                json!({"since": 2021}),
            ))
            .await?;

        Ok(())
    }

    async fn seed_two_hop_graph(pool: &sqlx::PgPool, fixture: &Fixture) -> Result<(), sqlx::Error> {
        let node_repository = NodeRepository::new(pool.clone());
        let edge_repository = EdgeRepository::new(pool.clone());

        let alice = sample_node(
            Uuid::from_u128(fixture.base + 21),
            vec!["Person"],
            json!({"name": fixture.alice.clone()}),
        );
        let bob = sample_node(
            Uuid::from_u128(fixture.base + 22),
            vec!["Person"],
            json!({"name": fixture.bob.clone()}),
        );
        let carol = sample_node(
            Uuid::from_u128(fixture.base + 23),
            vec!["Person"],
            json!({"name": fixture.carol.clone()}),
        );

        node_repository.insert(&alice).await?;
        node_repository.insert(&bob).await?;
        node_repository.insert(&carol).await?;

        edge_repository
            .insert(&sample_edge(
                Uuid::from_u128(fixture.base + 31),
                alice.id,
                bob.id,
                "KNOWS",
                json!({}),
            ))
            .await?;
        edge_repository
            .insert(&sample_edge(
                Uuid::from_u128(fixture.base + 32),
                carol.id,
                bob.id,
                "WORKS_WITH",
                json!({}),
            ))
            .await?;

        Ok(())
    }

    async fn test_pool() -> Result<Option<sqlx::PgPool>, sqlx::Error> {
        let database_url = match std::env::var("DATABASE_URL") {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };

        let pool = match PgPoolOptions::new()
            .max_connections(1)
            .connect(&database_url)
            .await
        {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!("skipping Postgres executor test setup: {error}");
                return Ok(None);
            }
        };

        if let Err(error) = MIGRATOR.run(&pool).await {
            eprintln!("skipping Postgres executor test migrations: {error}");
            return Ok(None);
        }

        Ok(Some(pool))
    }

    #[derive(Clone, Debug)]
    struct Fixture {
        base: u128,
        alice: String,
        bob: String,
        carol: String,
    }

    fn fixture(prefix: &str) -> Fixture {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(1);
        let suffix = format!("{prefix}-{nanos}");

        Fixture {
            base: nanos,
            alice: format!("Alice-{suffix}"),
            bob: format!("Bob-{suffix}"),
            carol: format!("Carol-{suffix}"),
        }
    }
}
