use std::collections::BTreeMap;

use pest::error::{Error as PestError, InputLocation, LineColLocation};
use pest::iterators::{Pair, Pairs};
use pest::{Parser, Span};
use pest_derive::Parser;

use crate::ast::{
    Clause, ComparisonOp, Expr, Literal, NodePattern, PathPattern, Pattern, PropertyAccess, Query,
    RelationshipDirection, RelationshipPattern, RelationshipStep, ReturnItem,
};

#[derive(Parser)]
#[grammar = "cypher.pest"]
pub struct CypherParser;

#[derive(Clone, Debug, PartialEq)]
pub struct ParserError {
    pub message: String,
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

pub fn parse_query(input: &str) -> Result<Pairs<'_, Rule>, PestError<Rule>> {
    CypherParser::parse(Rule::query, input)
}

pub fn parse_ast(input: &str) -> Result<Query, ParserError> {
    let mut pairs = parse_query(input).map_err(ParserError::from_pest)?;
    let query_pair = pairs
        .next()
        .ok_or_else(|| ParserError::new("expected query", 0, 0, 1, 1))?;

    lower_query(query_pair)
}

impl ParserError {
    fn new(
        message: impl Into<String>,
        start: usize,
        end: usize,
        line: usize,
        column: usize,
    ) -> Self {
        Self {
            message: message.into(),
            start,
            end,
            line,
            column,
        }
    }

    fn from_span(message: impl Into<String>, span: Span<'_>) -> Self {
        let (line, column) = span.start_pos().line_col();
        Self::new(message, span.start(), span.end(), line, column)
    }

    fn from_pest(error: PestError<Rule>) -> Self {
        let message = error.variant.message().to_owned();
        let (line, column) = match error.line_col {
            LineColLocation::Pos((line, column)) => (line, column),
            LineColLocation::Span((line, column), _) => (line, column),
        };

        match error.location {
            InputLocation::Pos(position) => Self::new(message, position, position, line, column),
            InputLocation::Span((start, end)) => Self::new(message, start, end, line, column),
        }
    }
}

fn lower_query(pair: Pair<'_, Rule>) -> Result<Query, ParserError> {
    let span = pair.as_span();
    if pair.as_rule() != Rule::query {
        return Err(ParserError::from_span("expected query", span));
    }

    let statement = pair
        .into_inner()
        .find(|inner| {
            matches!(
                inner.as_rule(),
                Rule::match_query | Rule::create_query | Rule::merge_query
            )
        })
        .ok_or_else(|| ParserError::from_span("expected statement", span))?;

    Ok(Query::Single(lower_statement(statement)?))
}

fn lower_statement(pair: Pair<'_, Rule>) -> Result<Vec<Clause>, ParserError> {
    let span = pair.as_span();

    match pair.as_rule() {
        Rule::statement => {
            let inner = pair
                .into_inner()
                .next()
                .ok_or_else(|| ParserError::from_span("statement is empty", span))?;
            lower_statement(inner)
        }
        Rule::match_query => lower_clause_sequence(pair, ClauseKind::Match),
        Rule::create_query => lower_clause_sequence(pair, ClauseKind::Create),
        Rule::merge_query => lower_clause_sequence(pair, ClauseKind::Merge),
        other => Err(ParserError::from_span(
            format!("expected statement, found {other:?}"),
            span,
        )),
    }
}

enum ClauseKind {
    Match,
    Create,
    Merge,
}

fn lower_clause_sequence(pair: Pair<'_, Rule>, kind: ClauseKind) -> Result<Vec<Clause>, ParserError> {
    let mut clauses = Vec::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::MATCH | Rule::CREATE | Rule::MERGE => {}
            Rule::pattern_list => {
                let patterns = lower_pattern_list(inner)?;
                clauses.push(match kind {
                    ClauseKind::Match => Clause::Match(patterns),
                    ClauseKind::Create => Clause::Create(patterns),
                    ClauseKind::Merge => Clause::Merge(patterns),
                });
            }
            Rule::where_clause => clauses.push(Clause::Where(lower_where(inner)?)),
            Rule::return_clause => clauses.push(Clause::Return(lower_return(inner)?)),
            Rule::limit_clause => clauses.push(Clause::Limit(lower_limit(inner)?)),
            other => {
                return Err(ParserError::from_span(
                    format!("unexpected clause component: {other:?}"),
                    inner.as_span(),
                ));
            }
        }
    }

    Ok(clauses)
}

fn lower_pattern_list(pair: Pair<'_, Rule>) -> Result<Vec<Pattern>, ParserError> {
    pair.into_inner()
        .filter(|inner| inner.as_rule() == Rule::pattern)
        .map(lower_pattern)
        .collect()
}

fn lower_pattern(pair: Pair<'_, Rule>) -> Result<Pattern, ParserError> {
    let span = pair.as_span();
    let mut inner = pair.into_inner();
    let start = inner
        .next()
        .ok_or_else(|| ParserError::from_span("expected starting node pattern", span))
        .and_then(lower_node_pattern)?;

    let mut steps = Vec::new();
    for item in inner {
        if item.as_rule() == Rule::relationship_step {
            steps.push(lower_relationship_step(item)?);
        }
    }

    Ok(Pattern::Path(PathPattern { start, steps }))
}

fn lower_relationship_step(pair: Pair<'_, Rule>) -> Result<RelationshipStep, ParserError> {
    let span = pair.as_span();
    let mut inner = pair.into_inner();
    let relationship = inner
        .next()
        .ok_or_else(|| ParserError::from_span("expected relationship pattern", span))
        .and_then(lower_relationship_pattern)?;
    let node = inner
        .next()
        .ok_or_else(|| ParserError::from_span("expected node pattern after relationship", span))
        .and_then(lower_node_pattern)?;

    Ok(RelationshipStep { relationship, node })
}

fn lower_node_pattern(pair: Pair<'_, Rule>) -> Result<NodePattern, ParserError> {
    let mut variable = None;
    let mut labels = Vec::new();
    let mut properties = None;

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::identifier if variable.is_none() => variable = Some(inner.as_str().to_owned()),
            Rule::node_labels => labels = lower_node_labels(inner),
            Rule::property_map => properties = Some(lower_property_map(inner)?),
            _ => {}
        }
    }

    Ok(NodePattern {
        variable,
        labels,
        properties,
    })
}

fn lower_node_labels(pair: Pair<'_, Rule>) -> Vec<String> {
    pair.into_inner()
        .filter(|inner| inner.as_rule() == Rule::label)
        .filter_map(|label| {
            label
                .into_inner()
                .find(|inner| inner.as_rule() == Rule::identifier)
                .map(|ident| ident.as_str().to_owned())
        })
        .collect()
}

fn lower_relationship_pattern(pair: Pair<'_, Rule>) -> Result<RelationshipPattern, ParserError> {
    let span = pair.as_span();
    let mut left = None;
    let mut right = None;
    let mut variable = None;
    let mut type_ = None;
    let mut properties = None;

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::relationship_left => left = Some(inner.as_str().to_owned()),
            Rule::relationship_right => right = Some(inner.as_str().to_owned()),
            Rule::relationship_detail => {
                for detail in inner.into_inner() {
                    match detail.as_rule() {
                        Rule::identifier if variable.is_none() => {
                            variable = Some(detail.as_str().to_owned())
                        }
                        Rule::relationship_type => {
                            type_ = detail
                                .into_inner()
                                .find(|inner| inner.as_rule() == Rule::identifier)
                                .map(|ident| ident.as_str().to_owned());
                        }
                        Rule::property_map => properties = Some(lower_property_map(detail)?),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    let direction = match (left.as_deref(), right.as_deref()) {
        (Some("-"), Some("->")) => RelationshipDirection::Right,
        (Some("<-"), Some("-")) => RelationshipDirection::Left,
        (Some("-"), Some("-")) => RelationshipDirection::Undirected,
        _ => return Err(ParserError::from_span("invalid relationship direction", span)),
    };

    Ok(RelationshipPattern {
        variable,
        type_,
        properties,
        direction,
    })
}

fn lower_property_map(pair: Pair<'_, Rule>) -> Result<BTreeMap<String, Literal>, ParserError> {
    let mut entries = BTreeMap::new();

    for property_pair in pair
        .into_inner()
        .filter(|inner| inner.as_rule() == Rule::property_pair)
    {
        let span = property_pair.as_span();
        let mut inner = property_pair.into_inner();
        let key = inner
            .next()
            .ok_or_else(|| ParserError::from_span("expected property key", span))?
            .as_str()
            .to_owned();
        let value = inner
            .next()
            .ok_or_else(|| ParserError::from_span("expected property value", span))
            .and_then(lower_literal)?;
        entries.insert(key, value);
    }

    Ok(entries)
}

fn lower_where(pair: Pair<'_, Rule>) -> Result<Expr, ParserError> {
    let span = pair.as_span();
    pair.into_inner()
        .find(|inner| inner.as_rule() == Rule::boolean_expr)
        .ok_or_else(|| ParserError::from_span("expected boolean expression", span))
        .and_then(lower_expr)
}

fn lower_return(pair: Pair<'_, Rule>) -> Result<Vec<ReturnItem>, ParserError> {
    pair.into_inner()
        .filter(|inner| inner.as_rule() == Rule::return_item)
        .map(lower_return_item)
        .collect()
}

fn lower_return_item(pair: Pair<'_, Rule>) -> Result<ReturnItem, ParserError> {
    let span = pair.as_span();
    let text = pair.as_str().trim();
    if text == "*" {
        return Ok(ReturnItem::All);
    }

    let inner = pair
        .into_inner()
        .next()
        .ok_or_else(|| ParserError::from_span("expected return item", span))?;

    match inner.as_rule() {
        Rule::property_access => Ok(ReturnItem::PropertyAccess(lower_property_access(inner)?)),
        Rule::identifier => Ok(ReturnItem::Identifier(inner.as_str().to_owned())),
        other => Err(ParserError::from_span(
            format!("unsupported return item: {other:?}"),
            inner.as_span(),
        )),
    }
}

fn lower_limit(pair: Pair<'_, Rule>) -> Result<u64, ParserError> {
    let span = pair.as_span();
    let integer = pair
        .into_inner()
        .find(|inner| inner.as_rule() == Rule::integer)
        .ok_or_else(|| ParserError::from_span("expected integer limit", span))?;

    integer
        .as_str()
        .parse::<u64>()
        .map_err(|_| ParserError::from_span("invalid limit value", integer.as_span()))
}

fn lower_expr(pair: Pair<'_, Rule>) -> Result<Expr, ParserError> {
    let span = pair.as_span();

    match pair.as_rule() {
        Rule::boolean_expr => {
            let inner = pair
                .into_inner()
                .next()
                .ok_or_else(|| ParserError::from_span("expected expression", span))?;
            lower_expr(inner)
        }
        Rule::or_expr => {
            let exprs = pair
                .into_inner()
                .filter(|inner| inner.as_rule() == Rule::and_expr)
                .map(lower_expr)
                .collect::<Result<Vec<_>, _>>()?;

            let mut exprs = exprs.into_iter();
            match exprs.len() {
                0 => Err(ParserError::from_span("expected OR expression", span)),
                1 => exprs
                    .next()
                    .ok_or_else(|| ParserError::from_span("expected OR expression", span)),
                _ => Ok(Expr::Or(exprs.collect())),
            }
        }
        Rule::and_expr => {
            let exprs = pair
                .into_inner()
                .filter(|inner| inner.as_rule() == Rule::not_expr)
                .map(lower_expr)
                .collect::<Result<Vec<_>, _>>()?;

            let mut exprs = exprs.into_iter();
            match exprs.len() {
                0 => Err(ParserError::from_span("expected AND expression", span)),
                1 => exprs
                    .next()
                    .ok_or_else(|| ParserError::from_span("expected AND expression", span)),
                _ => Ok(Expr::And(exprs.collect())),
            }
        }
        Rule::not_expr => {
            let mut not_count = 0usize;
            let mut predicate = None;

            for inner in pair.into_inner() {
                match inner.as_rule() {
                    Rule::NOT => not_count += 1,
                    Rule::predicate => predicate = Some(lower_expr(inner)?),
                    _ => {}
                }
            }

            let mut expr =
                predicate.ok_or_else(|| ParserError::from_span("expected predicate", span))?;
            for _ in 0..not_count {
                expr = Expr::Not(Box::new(expr));
            }
            Ok(expr)
        }
        Rule::predicate => {
            let inner = pair
                .into_inner()
                .next()
                .ok_or_else(|| ParserError::from_span("expected predicate inner expression", span))?;
            lower_expr(inner)
        }
        Rule::comparison => {
            let mut inner = pair.into_inner();
            let left = inner
                .next()
                .ok_or_else(|| ParserError::from_span("expected comparison left side", span))
                .and_then(lower_value_expr)?;
            let op = inner
                .next()
                .ok_or_else(|| ParserError::from_span("expected comparison operator", span))
                .and_then(lower_comparison_op)?;
            let right = inner
                .next()
                .ok_or_else(|| ParserError::from_span("expected comparison right side", span))
                .and_then(lower_value_expr)?;

            Ok(Expr::Comparison {
                left: Box::new(left),
                op,
                right: Box::new(right),
            })
        }
        other => Err(ParserError::from_span(
            format!("unsupported expression rule: {other:?}"),
            span,
        )),
    }
}

fn lower_comparison_op(pair: Pair<'_, Rule>) -> Result<ComparisonOp, ParserError> {
    let span = pair.as_span();
    let default_rule = pair.as_rule();
    let rule = pair
        .into_inner()
        .next()
        .map(|inner| inner.as_rule())
        .unwrap_or(default_rule);

    match rule {
        Rule::eq => Ok(ComparisonOp::Eq),
        Rule::neq => Ok(ComparisonOp::NotEq),
        Rule::gte => Ok(ComparisonOp::Gte),
        Rule::lte => Ok(ComparisonOp::Lte),
        Rule::gt => Ok(ComparisonOp::Gt),
        Rule::lt => Ok(ComparisonOp::Lt),
        other => Err(ParserError::from_span(
            format!("unsupported comparison operator: {other:?}"),
            span,
        )),
    }
}

fn lower_value_expr(pair: Pair<'_, Rule>) -> Result<Expr, ParserError> {
    let span = pair.as_span();
    let inner = match pair.as_rule() {
        Rule::value_expr => pair
            .into_inner()
            .next()
            .ok_or_else(|| ParserError::from_span("expected value expression", span))?,
        _ => pair,
    };

    match inner.as_rule() {
        Rule::identifier => Ok(Expr::Identifier(inner.as_str().to_owned())),
        Rule::property_access => Ok(Expr::PropertyAccess(lower_property_access(inner)?)),
        Rule::literal => Ok(Expr::Literal(lower_literal(inner)?)),
        other => Err(ParserError::from_span(
            format!("unsupported value expression: {other:?}"),
            inner.as_span(),
        )),
    }
}

fn lower_property_access(pair: Pair<'_, Rule>) -> Result<PropertyAccess, ParserError> {
    let span = pair.as_span();
    let segments = pair
        .into_inner()
        .filter(|inner| inner.as_rule() == Rule::identifier)
        .map(|inner| inner.as_str().to_owned())
        .collect::<Vec<_>>();

    if let Some((root, fields)) = segments.split_first() {
        Ok(PropertyAccess {
            root: root.clone(),
            fields: fields.to_vec(),
        })
    } else {
        Err(ParserError::from_span(
            "property access must contain at least one identifier",
            span,
        ))
    }
}

fn lower_literal(pair: Pair<'_, Rule>) -> Result<Literal, ParserError> {
    let span = pair.as_span();
    let inner = match pair.as_rule() {
        Rule::literal => pair
            .into_inner()
            .next()
            .ok_or_else(|| ParserError::from_span("expected literal", span))?,
        _ => pair,
    };

    match inner.as_rule() {
        Rule::null => Ok(Literal::Null),
        Rule::boolean => Ok(Literal::Boolean(inner.as_str().eq_ignore_ascii_case("true"))),
        Rule::number => lower_number(inner),
        Rule::string => Ok(Literal::String(decode_string(inner.as_str(), inner.as_span())?)),
        Rule::list => {
            let items = inner
                .into_inner()
                .filter(|item| item.as_rule() == Rule::literal)
                .map(lower_literal)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Literal::List(items))
        }
        Rule::property_map => Ok(Literal::Map(lower_property_map(inner)?)),
        other => Err(ParserError::from_span(
            format!("unsupported literal rule: {other:?}"),
            inner.as_span(),
        )),
    }
}

fn lower_number(pair: Pair<'_, Rule>) -> Result<Literal, ParserError> {
    let text = pair.as_str();
    if text.contains('.') {
        text.parse::<f64>()
            .map(Literal::Float)
            .map_err(|_| ParserError::from_span("invalid float literal", pair.as_span()))
    } else {
        text.parse::<i64>()
            .map(Literal::Integer)
            .map_err(|_| ParserError::from_span("invalid integer literal", pair.as_span()))
    }
}

fn decode_string(raw: &str, span: Span<'_>) -> Result<String, ParserError> {
    let mut chars = raw[1..raw.len() - 1].chars();
    let mut decoded = String::new();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            let escaped = chars
                .next()
                .ok_or_else(|| ParserError::from_span("unterminated string escape", span))?;
            match escaped {
                '"' => decoded.push('"'),
                '\\' => decoded.push('\\'),
                '/' => decoded.push('/'),
                'b' => decoded.push('\u{0008}'),
                'f' => decoded.push('\u{000C}'),
                'n' => decoded.push('\n'),
                'r' => decoded.push('\r'),
                't' => decoded.push('\t'),
                other => {
                    return Err(ParserError::from_span(
                        format!("unsupported string escape: \\{other}"),
                        span,
                    ));
                }
            }
        } else {
            decoded.push(ch);
        }
    }

    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use pest::Parser;

    use crate::ast::{
        Clause, ComparisonOp, Expr, Literal, Pattern, Query, RelationshipDirection, ReturnItem,
    };

    use super::{parse_ast, parse_query, CypherParser, Rule};

    #[test]
    fn parses_match_where_return_limit_with_two_hops() {
        let query = r#"
            MATCH (a:Person {name: "Alice"})-[r:KNOWS]->(b:Person)<-[s:WORKS_WITH]-(c)
            WHERE a.name = "Alice" AND (b.age >= 30 OR NOT c.active = false)
            RETURN a, b.name, c
            LIMIT 10
        "#;

        let pairs = parse_query(query).expect("query should parse");
        assert_eq!(pairs.as_str().trim(), query.trim());
    }

    #[test]
    fn parses_create_and_merge_statements() {
        let create = r#"CREATE (a:Person {name: "Alice"})-[:KNOWS]->(b:Person) RETURN a, b"#;
        let merge = r#"MERGE (n:Person {email: "alice@example.com"}) RETURN n"#;

        assert!(parse_query(create).is_ok());
        assert!(parse_query(merge).is_ok());
    }

    #[test]
    fn parses_property_map_rule() {
        let property_map = r#"{name: "Alice", age: 30, active: true}"#;
        let pairs = CypherParser::parse(Rule::property_map, property_map)
            .expect("property map should parse");

        assert_eq!(pairs.as_str(), property_map);
    }

    #[test]
    fn parses_all_literal_kinds() {
        for literal in [
            r#""Alice""#,
            "42",
            "-3.14",
            "true",
            "false",
            "null",
            r#"["a", 1, false]"#,
            r#"{name: "Alice"}"#,
        ] {
            let pairs = CypherParser::parse(Rule::literal, literal)
                .expect("literal should parse");
            assert_eq!(pairs.as_str(), literal);
        }
    }

    #[test]
    fn parses_identifier_and_property_access_rules() {
        let identifier = CypherParser::parse(Rule::identifier, "person_1")
            .expect("identifier should parse");
        let property_access = CypherParser::parse(Rule::property_access, "person.profile.name")
            .expect("property access should parse");

        assert_eq!(identifier.as_str(), "person_1");
        assert_eq!(property_access.as_str(), "person.profile.name");
    }

    #[test]
    fn parses_boolean_and_comparison_operator_rules() {
        let boolean_expr = r#"NOT person.active = false AND person.age >= 21 OR person.score < 100"#;
        let pairs = CypherParser::parse(Rule::boolean_expr, boolean_expr)
            .expect("boolean expression should parse");

        assert_eq!(pairs.as_str(), boolean_expr);
    }

    #[test]
    fn parses_return_star_rule() {
        let pairs = CypherParser::parse(Rule::return_clause, "RETURN *")
            .expect("return star should parse");

        assert_eq!(pairs.as_str(), "RETURN *");
    }

    #[test]
    fn parses_node_and_relationship_pattern_variants() {
        for pattern in [
            "(n:Person)",
            "(n)-[:KNOWS]->(m)",
            "(n)<-[:KNOWS]-(m)",
            "(n)-[:KNOWS]-(m)",
        ] {
            let pairs = CypherParser::parse(Rule::pattern, pattern)
                .expect("pattern should parse");
            assert_eq!(pairs.as_str(), pattern);
        }
    }

    #[test]
    fn lowers_match_query_into_ast() {
        let query = r#"
            MATCH (a:Person {name: "Alice"})-[r:KNOWS]->(b:Person)<-[s:WORKS_WITH]-(c)
            WHERE a.name = "Alice" AND (b.age >= 30 OR NOT c.active = false)
            RETURN a, b.name, c
            LIMIT 10
        "#;

        let ast = parse_ast(query).expect("query should lower");

        match ast {
            Query::Single(clauses) => {
                assert!(matches!(clauses[0], Clause::Match(_)));
                assert!(matches!(clauses[1], Clause::Where(_)));
                assert!(matches!(clauses[2], Clause::Return(_)));
                assert_eq!(clauses[3], Clause::Limit(10));

                match &clauses[0] {
                    Clause::Match(patterns) => match &patterns[0] {
                        Pattern::Path(path) => {
                            assert_eq!(path.start.variable.as_deref(), Some("a"));
                            assert_eq!(path.steps.len(), 2);
                            assert_eq!(
                                path.steps[0].relationship.direction,
                                RelationshipDirection::Right
                            );
                            assert_eq!(
                                path.steps[1].relationship.direction,
                                RelationshipDirection::Left
                            );
                        }
                    },
                    _ => unreachable!(),
                }

                match &clauses[1] {
                    Clause::Where(Expr::And(parts)) => {
                        assert_eq!(parts.len(), 2);
                        match &parts[0] {
                            Expr::Comparison { op, .. } => assert_eq!(*op, ComparisonOp::Eq),
                            _ => panic!("expected comparison"),
                        }
                    }
                    _ => panic!("expected where clause"),
                }

                match &clauses[2] {
                    Clause::Return(items) => {
                        assert_eq!(items.len(), 3);
                        assert_eq!(items[0], ReturnItem::Identifier("a".to_owned()));
                    }
                    _ => panic!("expected return clause"),
                }
            }
        }
    }

    #[test]
    fn lowers_create_and_merge_literals() {
        let create = parse_ast(r#"CREATE (a {tags: ["a", "b"], active: true}) RETURN a"#)
            .expect("create should lower");
        let merge = parse_ast(r#"MERGE (a)-[r:LIKES {score: 3.14}]->(b) RETURN r"#)
            .expect("merge should lower");

        match create {
            Query::Single(clauses) => match &clauses[0] {
                Clause::Create(patterns) => match &patterns[0] {
                    Pattern::Path(path) => {
                        let props = path.start.properties.as_ref().expect("properties");
                        assert_eq!(props.get("active"), Some(&Literal::Boolean(true)));
                    }
                },
                _ => panic!("expected create clause"),
            },
        }

        match merge {
            Query::Single(clauses) => match &clauses[0] {
                Clause::Merge(patterns) => match &patterns[0] {
                    Pattern::Path(path) => {
                        let rel = &path.steps[0].relationship;
                        assert_eq!(rel.type_.as_deref(), Some("LIKES"));
                    }
                },
                _ => panic!("expected merge clause"),
            },
        }
    }

    #[test]
    fn lowers_return_star_and_undirected_relationship() {
        let query = parse_ast(r#"MATCH (a)-[r:KNOWS]-(b) RETURN *"#)
            .expect("query should lower");

        match query {
            Query::Single(clauses) => {
                match &clauses[0] {
                    Clause::Match(patterns) => match &patterns[0] {
                        Pattern::Path(path) => {
                            assert_eq!(path.steps.len(), 1);
                            assert_eq!(
                                path.steps[0].relationship.direction,
                                RelationshipDirection::Undirected
                            );
                        }
                    },
                    _ => panic!("expected match clause"),
                }

                match &clauses[1] {
                    Clause::Return(items) => assert_eq!(items, &vec![ReturnItem::All]),
                    _ => panic!("expected return clause"),
                }
            }
        }
    }

    #[test]
    fn lowers_nested_property_maps_and_lists() {
        let query = parse_ast(
            r#"CREATE (n {profile: {name: "Alice"}, tags: ["a", "b"], score: -3.5}) RETURN n"#,
        )
        .expect("query should lower");

        match query {
            Query::Single(clauses) => match &clauses[0] {
                Clause::Create(patterns) => match &patterns[0] {
                    Pattern::Path(path) => {
                        let props = path.start.properties.as_ref().expect("properties");
                        assert_eq!(
                            props.get("score"),
                            Some(&Literal::Float(-3.5))
                        );
                        assert_eq!(
                            props.get("tags"),
                            Some(&Literal::List(vec![
                                Literal::String("a".to_owned()),
                                Literal::String("b".to_owned()),
                            ]))
                        );
                        assert!(matches!(props.get("profile"), Some(Literal::Map(_))));
                    }
                },
                _ => panic!("expected create clause"),
            },
        }
    }

    #[test]
    fn rejects_invalid_string_escape() {
        let error = parse_ast(r#"CREATE (n {name: "\u"}) RETURN n"#)
            .expect_err("query should fail");

        assert!(error.message.contains("unsupported"));
    }

    #[test]
    fn parser_error_includes_span_information() {
        let error = parse_ast(r#"MATCH (n) WHERE n.name = "Alice""#).expect_err("query should fail");

        assert!(error.start <= error.end);
        assert!(error.line >= 1);
        assert!(error.column >= 1);
    }

    #[test]
    fn rejects_path_longer_than_two_relationships() {
        let query = r#"
            MATCH (a)-[:R1]->(b)-[:R2]->(c)-[:R3]->(d)
            RETURN d
        "#;

        let error = parse_ast(query).expect_err("query should fail");
        assert!(error.start <= error.end);
    }
}
