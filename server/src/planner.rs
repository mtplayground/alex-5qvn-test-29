use std::collections::BTreeMap;

use crate::ast::{
    Clause, Expr, Literal, Pattern, PropertyAccess, Query, RelationshipDirection, ReturnItem,
};

#[derive(Clone, Debug, PartialEq)]
pub struct LogicalPlan {
    pub steps: Vec<PlanStep>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlanStep {
    NodeScan(NodeScan),
    Expand(EdgeExpand),
    CreateNode(CreateNode),
    CreateEdge(CreateEdge),
    MergeNode(MergeNode),
    Filter(Expr),
    Project(Projection),
    Limit(u64),
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeScan {
    pub binding: String,
    pub labels: Vec<String>,
    pub properties: BTreeMap<String, Literal>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EdgeExpand {
    pub from_binding: String,
    pub edge_binding: String,
    pub edge_type: Option<String>,
    pub edge_properties: BTreeMap<String, Literal>,
    pub direction: RelationshipDirection,
    pub to_binding: String,
    pub to_labels: Vec<String>,
    pub to_properties: BTreeMap<String, Literal>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CreateNode {
    pub binding: String,
    pub labels: Vec<String>,
    pub properties: BTreeMap<String, Literal>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CreateEdge {
    pub from_binding: String,
    pub edge_binding: String,
    pub edge_type: String,
    pub edge_properties: BTreeMap<String, Literal>,
    pub direction: RelationshipDirection,
    pub to_binding: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MergeNode {
    pub binding: String,
    pub label: String,
    pub key: String,
    pub value: Literal,
    pub properties: BTreeMap<String, Literal>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Projection {
    pub items: Vec<ProjectionItem>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProjectionItem {
    All,
    Identifier(String),
    PropertyAccess(PropertyAccess),
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlannerError {
    pub message: String,
}

impl PlannerError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

pub fn plan_query(query: &Query) -> Result<LogicalPlan, PlannerError> {
    Planner::default().plan_query(query)
}

#[derive(Default)]
struct Planner {
    next_node_binding: usize,
    next_edge_binding: usize,
}

impl Planner {
    fn plan_query(&mut self, query: &Query) -> Result<LogicalPlan, PlannerError> {
        match query {
            Query::Single(clauses) => self.plan_clauses(clauses),
        }
    }

    fn plan_clauses(&mut self, clauses: &[Clause]) -> Result<LogicalPlan, PlannerError> {
        let mut steps = Vec::new();

        for clause in clauses {
            match clause {
                Clause::Match(patterns) => {
                    for pattern in patterns {
                        self.plan_pattern(pattern, &mut steps)?;
                    }
                }
                Clause::Create(patterns) => {
                    for pattern in patterns {
                        self.plan_create_pattern(pattern, &mut steps)?;
                    }
                }
                Clause::Merge(patterns) => {
                    for pattern in patterns {
                        self.plan_merge_pattern(pattern, &mut steps)?;
                    }
                }
                Clause::Where(expr) => steps.push(PlanStep::Filter(expr.clone())),
                Clause::Return(items) => steps.push(PlanStep::Project(Projection {
                    items: items.iter().cloned().map(ProjectionItem::from).collect(),
                })),
                Clause::Limit(limit) => steps.push(PlanStep::Limit(*limit)),
            }
        }

        Ok(LogicalPlan { steps })
    }

    fn plan_pattern(
        &mut self,
        pattern: &Pattern,
        steps: &mut Vec<PlanStep>,
    ) -> Result<(), PlannerError> {
        match pattern {
            Pattern::Path(path) => {
                let start_binding = self.node_binding(path.start.variable.as_deref());
                steps.push(PlanStep::NodeScan(NodeScan {
                    binding: start_binding.clone(),
                    labels: path.start.labels.clone(),
                    properties: path.start.properties.clone().unwrap_or_default(),
                }));

                let mut from_binding = start_binding;
                for step in &path.steps {
                    let edge_binding =
                        self.edge_binding(step.relationship.variable.as_deref());
                    let to_binding = self.node_binding(step.node.variable.as_deref());

                    steps.push(PlanStep::Expand(EdgeExpand {
                        from_binding: from_binding.clone(),
                        edge_binding,
                        edge_type: step.relationship.type_.clone(),
                        edge_properties: step
                            .relationship
                            .properties
                            .clone()
                            .unwrap_or_default(),
                        direction: step.relationship.direction.clone(),
                        to_binding: to_binding.clone(),
                        to_labels: step.node.labels.clone(),
                        to_properties: step.node.properties.clone().unwrap_or_default(),
                    }));

                    from_binding = to_binding;
                }

                Ok(())
            }
        }
    }

    fn plan_create_pattern(
        &mut self,
        pattern: &Pattern,
        steps: &mut Vec<PlanStep>,
    ) -> Result<(), PlannerError> {
        match pattern {
            Pattern::Path(path) => {
                if path.steps.len() > 1 {
                    return Err(PlannerError::new(
                        "CREATE currently supports node and single-edge patterns only",
                    ));
                }

                let start_binding = self.node_binding(path.start.variable.as_deref());
                steps.push(PlanStep::CreateNode(CreateNode {
                    binding: start_binding.clone(),
                    labels: path.start.labels.clone(),
                    properties: path.start.properties.clone().unwrap_or_default(),
                }));

                if let Some(step) = path.steps.first() {
                    let to_binding = self.node_binding(step.node.variable.as_deref());
                    steps.push(PlanStep::CreateNode(CreateNode {
                        binding: to_binding.clone(),
                        labels: step.node.labels.clone(),
                        properties: step.node.properties.clone().unwrap_or_default(),
                    }));

                    let edge_type = step.relationship.type_.clone().ok_or_else(|| {
                        PlannerError::new("CREATE relationship pattern requires a type")
                    })?;
                    let edge_binding = self.edge_binding(step.relationship.variable.as_deref());

                    steps.push(PlanStep::CreateEdge(CreateEdge {
                        from_binding: start_binding,
                        edge_binding,
                        edge_type,
                        edge_properties: step
                            .relationship
                            .properties
                            .clone()
                            .unwrap_or_default(),
                        direction: step.relationship.direction.clone(),
                        to_binding,
                    }));
                }

                Ok(())
            }
        }
    }

    fn plan_merge_pattern(
        &mut self,
        pattern: &Pattern,
        steps: &mut Vec<PlanStep>,
    ) -> Result<(), PlannerError> {
        match pattern {
            Pattern::Path(path) => {
                if !path.steps.is_empty() {
                    return Err(PlannerError::new(
                        "MERGE currently supports a single node pattern only",
                    ));
                }

                let binding = self.node_binding(path.start.variable.as_deref());
                let label = path
                    .start
                    .labels
                    .first()
                    .cloned()
                    .ok_or_else(|| PlannerError::new("MERGE node pattern requires one label"))?;
                if path.start.labels.len() != 1 {
                    return Err(PlannerError::new(
                        "MERGE currently supports exactly one node label",
                    ));
                }

                let properties = path.start.properties.clone().ok_or_else(|| {
                    PlannerError::new("MERGE node pattern requires one unique property")
                })?;
                if properties.len() != 1 {
                    return Err(PlannerError::new(
                        "MERGE currently supports exactly one unique property",
                    ));
                }

                let (key, value) = properties
                    .iter()
                    .next()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .ok_or_else(|| PlannerError::new("MERGE node pattern requires one property"))?;

                steps.push(PlanStep::MergeNode(MergeNode {
                    binding,
                    label,
                    key,
                    value,
                    properties,
                }));

                Ok(())
            }
        }
    }

    fn node_binding(&mut self, binding: Option<&str>) -> String {
        binding
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| self.synthetic_node_binding())
    }

    fn edge_binding(&mut self, binding: Option<&str>) -> String {
        binding
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| self.synthetic_edge_binding())
    }

    fn synthetic_node_binding(&mut self) -> String {
        let binding = format!("_node{}", self.next_node_binding);
        self.next_node_binding += 1;
        binding
    }

    fn synthetic_edge_binding(&mut self) -> String {
        let binding = format!("_edge{}", self.next_edge_binding);
        self.next_edge_binding += 1;
        binding
    }
}

impl From<ReturnItem> for ProjectionItem {
    fn from(value: ReturnItem) -> Self {
        match value {
            ReturnItem::All => ProjectionItem::All,
            ReturnItem::Identifier(identifier) => ProjectionItem::Identifier(identifier),
            ReturnItem::PropertyAccess(access) => ProjectionItem::PropertyAccess(access),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::ast::{ComparisonOp, Expr, Literal};
    use crate::parser::parse_ast;

    use super::{
        plan_query, CreateEdge, CreateNode, EdgeExpand, LogicalPlan, MergeNode, NodeScan,
        PlanStep, PlannerError, Projection, ProjectionItem,
    };

    #[test]
    fn plans_zero_hop_match_query() {
        let query = parse_ast(r#"MATCH (n:Person {name: "Alice"}) RETURN n LIMIT 5"#)
            .expect("query should parse");

        let plan = plan_query(&query).expect("query should plan");

        assert_eq!(
            plan,
            LogicalPlan {
                steps: vec![
                    PlanStep::NodeScan(NodeScan {
                        binding: "n".to_owned(),
                        labels: vec!["Person".to_owned()],
                        properties: BTreeMap::from([(
                            "name".to_owned(),
                            Literal::String("Alice".to_owned()),
                        )]),
                    }),
                    PlanStep::Project(Projection {
                        items: vec![ProjectionItem::Identifier("n".to_owned())],
                    }),
                    PlanStep::Limit(5),
                ],
            }
        );
    }

    #[test]
    fn plans_one_hop_match_query_with_filter_projection_and_limit() {
        let query = parse_ast(
            r#"
            MATCH (n:Person {name: "Alice"})-[r:KNOWS {since: 2020}]->(m:Person)
            WHERE m.age >= 30
            RETURN n, m.name
            LIMIT 10
            "#,
        )
        .expect("query should parse");

        let plan = plan_query(&query).expect("query should plan");

        assert_eq!(plan.steps.len(), 5);
        assert_eq!(
            plan.steps[0],
            PlanStep::NodeScan(NodeScan {
                binding: "n".to_owned(),
                labels: vec!["Person".to_owned()],
                properties: BTreeMap::from([(
                    "name".to_owned(),
                    Literal::String("Alice".to_owned()),
                )]),
            })
        );
        assert_eq!(
            plan.steps[1],
            PlanStep::Expand(EdgeExpand {
                from_binding: "n".to_owned(),
                edge_binding: "r".to_owned(),
                edge_type: Some("KNOWS".to_owned()),
                edge_properties: BTreeMap::from([(
                    "since".to_owned(),
                    Literal::Integer(2020),
                )]),
                direction: crate::ast::RelationshipDirection::Right,
                to_binding: "m".to_owned(),
                to_labels: vec!["Person".to_owned()],
                to_properties: BTreeMap::new(),
            })
        );
        assert_eq!(
            plan.steps[2],
            PlanStep::Filter(Expr::Comparison {
                left: Box::new(Expr::PropertyAccess(crate::ast::PropertyAccess {
                    root: "m".to_owned(),
                    fields: vec!["age".to_owned()],
                })),
                op: ComparisonOp::Gte,
                right: Box::new(Expr::Literal(Literal::Integer(30))),
            })
        );
        assert_eq!(
            plan.steps[3],
            PlanStep::Project(Projection {
                items: vec![
                    ProjectionItem::Identifier("n".to_owned()),
                    ProjectionItem::PropertyAccess(crate::ast::PropertyAccess {
                        root: "m".to_owned(),
                        fields: vec!["name".to_owned()],
                    }),
                ],
            })
        );
        assert_eq!(plan.steps[4], PlanStep::Limit(10));
    }

    #[test]
    fn plans_two_hop_match_query_with_synthetic_bindings() {
        let query = parse_ast(
            r#"
            MATCH (:Person)-[:KNOWS]->(friend:Person)<-[:WORKS_WITH]-(coworker)
            RETURN friend, coworker
            "#,
        )
        .expect("query should parse");

        let plan = plan_query(&query).expect("query should plan");

        assert_eq!(
            plan.steps,
            vec![
                PlanStep::NodeScan(NodeScan {
                    binding: "_node0".to_owned(),
                    labels: vec!["Person".to_owned()],
                    properties: BTreeMap::new(),
                }),
                PlanStep::Expand(EdgeExpand {
                    from_binding: "_node0".to_owned(),
                    edge_binding: "_edge0".to_owned(),
                    edge_type: Some("KNOWS".to_owned()),
                    edge_properties: BTreeMap::new(),
                    direction: crate::ast::RelationshipDirection::Right,
                    to_binding: "friend".to_owned(),
                    to_labels: vec!["Person".to_owned()],
                    to_properties: BTreeMap::new(),
                }),
                PlanStep::Expand(EdgeExpand {
                    from_binding: "friend".to_owned(),
                    edge_binding: "_edge1".to_owned(),
                    edge_type: Some("WORKS_WITH".to_owned()),
                    edge_properties: BTreeMap::new(),
                    direction: crate::ast::RelationshipDirection::Left,
                    to_binding: "coworker".to_owned(),
                    to_labels: Vec::new(),
                    to_properties: BTreeMap::new(),
                }),
                PlanStep::Project(Projection {
                    items: vec![
                        ProjectionItem::Identifier("friend".to_owned()),
                        ProjectionItem::Identifier("coworker".to_owned()),
                    ],
                }),
            ]
        );
    }

    #[test]
    fn plans_create_node_and_edge_queries() {
        let query = parse_ast(
            r#"
            CREATE (a:Person {name: "Alice"})-[r:KNOWS {since: 2024}]->(b:Person {name: "Bob"})
            RETURN a, r, b
            "#,
        )
        .expect("query should parse");

        let plan = plan_query(&query).expect("create should plan");

        assert_eq!(
            plan.steps,
            vec![
                PlanStep::CreateNode(CreateNode {
                    binding: "a".to_owned(),
                    labels: vec!["Person".to_owned()],
                    properties: BTreeMap::from([(
                        "name".to_owned(),
                        Literal::String("Alice".to_owned()),
                    )]),
                }),
                PlanStep::CreateNode(CreateNode {
                    binding: "b".to_owned(),
                    labels: vec!["Person".to_owned()],
                    properties: BTreeMap::from([(
                        "name".to_owned(),
                        Literal::String("Bob".to_owned()),
                    )]),
                }),
                PlanStep::CreateEdge(CreateEdge {
                    from_binding: "a".to_owned(),
                    edge_binding: "r".to_owned(),
                    edge_type: "KNOWS".to_owned(),
                    edge_properties: BTreeMap::from([(
                        "since".to_owned(),
                        Literal::Integer(2024),
                    )]),
                    direction: crate::ast::RelationshipDirection::Right,
                    to_binding: "b".to_owned(),
                }),
                PlanStep::Project(Projection {
                    items: vec![
                        ProjectionItem::Identifier("a".to_owned()),
                        ProjectionItem::Identifier("r".to_owned()),
                        ProjectionItem::Identifier("b".to_owned()),
                    ],
                }),
            ]
        );
    }

    #[test]
    fn plans_merge_node_query() {
        let query = parse_ast(r#"MERGE (n:Person {email: "alice@example.com"}) RETURN n"#)
            .expect("query should parse");

        let plan = plan_query(&query).expect("merge should plan");

        assert_eq!(
            plan.steps,
            vec![
                PlanStep::MergeNode(MergeNode {
                    binding: "n".to_owned(),
                    label: "Person".to_owned(),
                    key: "email".to_owned(),
                    value: Literal::String("alice@example.com".to_owned()),
                    properties: BTreeMap::from([(
                        "email".to_owned(),
                        Literal::String("alice@example.com".to_owned()),
                    )]),
                }),
                PlanStep::Project(Projection {
                    items: vec![ProjectionItem::Identifier("n".to_owned())],
                }),
            ]
        );
    }

    #[test]
    fn rejects_unsupported_write_patterns() {
        let create = parse_ast(r#"CREATE (a)-[:R1]->(b)-[:R2]->(c) RETURN a"#)
            .expect("query should parse");
        let merge = parse_ast(r#"MERGE (n:Person {email: "a", id: 1}) RETURN n"#)
            .expect("query should parse");

        let create_error = plan_query(&create).expect_err("create should not plan");
        let merge_error = plan_query(&merge).expect_err("merge should not plan");

        assert_eq!(
            create_error,
            PlannerError {
                message: "CREATE currently supports node and single-edge patterns only".to_owned(),
            }
        );
        assert_eq!(
            merge_error,
            PlannerError {
                message: "MERGE currently supports exactly one unique property".to_owned(),
            }
        );
    }
}
