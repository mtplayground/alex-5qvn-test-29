use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Query {
    Single(Vec<Clause>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Clause {
    Match(Vec<Pattern>),
    Create(Vec<Pattern>),
    Merge(Vec<Pattern>),
    Where(Expr),
    Return(Vec<ReturnItem>),
    Limit(u64),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Pattern {
    Path(PathPattern),
}

#[derive(Clone, Debug, PartialEq)]
pub struct PathPattern {
    pub start: NodePattern,
    pub steps: Vec<RelationshipStep>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodePattern {
    pub variable: Option<String>,
    pub labels: Vec<String>,
    pub properties: Option<BTreeMap<String, Literal>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RelationshipStep {
    pub relationship: RelationshipPattern,
    pub node: NodePattern,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RelationshipPattern {
    pub variable: Option<String>,
    pub type_: Option<String>,
    pub properties: Option<BTreeMap<String, Literal>>,
    pub direction: RelationshipDirection,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RelationshipDirection {
    Left,
    Right,
    Undirected,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ReturnItem {
    All,
    Identifier(String),
    PropertyAccess(PropertyAccess),
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropertyAccess {
    pub root: String,
    pub fields: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Identifier(String),
    PropertyAccess(PropertyAccess),
    Literal(Literal),
    Comparison {
        left: Box<Expr>,
        op: ComparisonOp,
        right: Box<Expr>,
    },
    And(Vec<Expr>),
    Or(Vec<Expr>),
    Not(Box<Expr>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ComparisonOp {
    Eq,
    NotEq,
    Gte,
    Lte,
    Gt,
    Lt,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Literal {
    Null,
    Boolean(bool),
    Integer(i64),
    Float(f64),
    String(String),
    List(Vec<Literal>),
    Map(BTreeMap<String, Literal>),
}
