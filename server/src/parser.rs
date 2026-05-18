use pest::error::Error;
use pest::iterators::Pairs;
use pest::Parser;
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "cypher.pest"]
pub struct CypherParser;

pub fn parse_query(input: &str) -> Result<Pairs<'_, Rule>, Error<Rule>> {
    CypherParser::parse(Rule::query, input)
}

#[cfg(test)]
mod tests {
    use super::{parse_query, CypherParser, Rule};
    use pest::Parser;

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
    fn parses_create_and_merge_patterns() {
        let create_query = r#"CREATE (a:Person {name: "Alice"})-[:KNOWS]->(b:Person {name: "Bob"})"#;
        let merge_query = r#"MERGE (a)-[r:LIKES {since: 2024}]->(b) RETURN r"#;

        assert!(parse_query(create_query).is_ok());
        assert!(parse_query(merge_query).is_ok());
    }

    #[test]
    fn parses_literals_property_maps_and_boolean_operators() {
        let query = r#"
            MATCH (n {name: "Alice", age: 42, active: true, tags: ["a", "b"], score: 3.14})
            WHERE NOT n.active = false OR n.age > 40
            RETURN n
        "#;

        assert!(parse_query(query).is_ok());
    }

    #[test]
    fn rejects_path_longer_than_two_relationships() {
        let query = r#"
            MATCH (a)-[:R1]->(b)-[:R2]->(c)-[:R3]->(d)
            RETURN d
        "#;

        assert!(CypherParser::parse(Rule::query, query).is_err());
    }

    #[test]
    fn rejects_missing_return_in_match_query() {
        let query = r#"MATCH (n) WHERE n.name = "Alice""#;

        assert!(parse_query(query).is_err());
    }
}
