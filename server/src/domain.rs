use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlx::postgres::PgRow;
use sqlx::types::Json;
use sqlx::{FromRow, Row};
use uuid::Uuid;

pub type Properties = Map<String, Value>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Node {
    pub id: Uuid,
    pub labels: Vec<String>,
    pub properties: Properties,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Edge {
    pub id: Uuid,
    pub start_id: Uuid,
    pub end_id: Uuid,
    #[serde(rename = "type")]
    pub type_: String,
    pub properties: Properties,
}

impl<'r> FromRow<'r, PgRow> for Node {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            labels: row.try_get("labels")?,
            properties: decode_properties(row, "properties")?,
        })
    }
}

impl<'r> FromRow<'r, PgRow> for Edge {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            start_id: row.try_get("start_id")?,
            end_id: row.try_get("end_id")?,
            type_: row.try_get("type")?,
            properties: decode_properties(row, "properties")?,
        })
    }
}

fn decode_properties(row: &PgRow, column: &str) -> Result<Properties, sqlx::Error> {
    let Json(properties) = row.try_get::<Json<Properties>, _>(column)?;
    Ok(properties)
}

#[cfg(test)]
mod tests {
    use super::{Edge, Node, Properties};
    use serde_json::{json, Value};
    use uuid::Uuid;

    fn properties() -> Properties {
        match json!({
            "name": "alpha",
            "weight": 3
        }) {
            Value::Object(map) => map,
            _ => unreachable!(),
        }
    }

    #[test]
    fn node_serializes_to_expected_shape() {
        let node = Node {
            id: Uuid::nil(),
            labels: vec!["Person".to_owned(), "Admin".to_owned()],
            properties: properties(),
        };

        let json = serde_json::to_value(node).expect("node should serialize");

        assert_eq!(
            json,
            json!({
                "id": "00000000-0000-0000-0000-000000000000",
                "labels": ["Person", "Admin"],
                "properties": {
                    "name": "alpha",
                    "weight": 3
                }
            })
        );
    }

    #[test]
    fn edge_serializes_type_field_without_suffix() {
        let edge = Edge {
            id: Uuid::nil(),
            start_id: Uuid::nil(),
            end_id: Uuid::from_u128(1),
            type_: "KNOWS".to_owned(),
            properties: properties(),
        };

        let json = serde_json::to_value(edge).expect("edge should serialize");

        assert_eq!(
            json,
            json!({
                "id": "00000000-0000-0000-0000-000000000000",
                "start_id": "00000000-0000-0000-0000-000000000000",
                "end_id": "00000000-0000-0000-0000-000000000001",
                "type": "KNOWS",
                "properties": {
                    "name": "alpha",
                    "weight": 3
                }
            })
        );
    }
}
