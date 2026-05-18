use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlx::error::BoxDynError;
use sqlx::sqlite::SqliteRow;
use sqlx::{FromRow, Row};
use uuid::Uuid;

pub type Properties = Map<String, Value>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, FromRow)]
pub struct LabelCount {
    pub label: String,
    pub count: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, FromRow)]
pub struct RelationshipTypeCount {
    #[serde(rename = "type")]
    pub type_: String,
    pub count: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SchemaCatalog {
    pub labels: Vec<LabelCount>,
    pub relationship_types: Vec<RelationshipTypeCount>,
}

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

impl<'r> FromRow<'r, SqliteRow> for Node {
    fn from_row(row: &'r SqliteRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: decode_uuid(row, "id")?,
            labels: decode_json(row, "labels")?,
            properties: decode_properties(row, "properties")?,
        })
    }
}

impl<'r> FromRow<'r, SqliteRow> for Edge {
    fn from_row(row: &'r SqliteRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: decode_uuid(row, "id")?,
            start_id: decode_uuid(row, "start_id")?,
            end_id: decode_uuid(row, "end_id")?,
            type_: row.try_get("type")?,
            properties: decode_properties(row, "properties")?,
        })
    }
}

fn decode_uuid(row: &SqliteRow, column: &str) -> Result<Uuid, sqlx::Error> {
    let value: String = row.try_get(column)?;
    Uuid::parse_str(&value).map_err(|error| sqlx::Error::Decode(Box::new(error) as BoxDynError))
}

fn decode_properties(row: &SqliteRow, column: &str) -> Result<Properties, sqlx::Error> {
    decode_json(row, column)
}

fn decode_json<T>(row: &SqliteRow, column: &str) -> Result<T, sqlx::Error>
where
    T: serde::de::DeserializeOwned,
{
    let value: String = row.try_get(column)?;
    serde_json::from_str(&value)
        .map_err(|error| sqlx::Error::Decode(Box::new(error) as BoxDynError))
}

#[cfg(test)]
mod tests {
    use super::{Edge, LabelCount, Node, Properties, RelationshipTypeCount, SchemaCatalog};
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

    #[test]
    fn schema_catalog_serializes_label_and_type_counts() {
        let catalog = SchemaCatalog {
            labels: vec![LabelCount {
                label: "Person".to_owned(),
                count: 3,
            }],
            relationship_types: vec![RelationshipTypeCount {
                type_: "KNOWS".to_owned(),
                count: 2,
            }],
        };

        let json = serde_json::to_value(catalog).expect("catalog should serialize");

        assert_eq!(
            json,
            json!({
                "labels": [
                    { "label": "Person", "count": 3 }
                ],
                "relationship_types": [
                    { "type": "KNOWS", "count": 2 }
                ]
            })
        );
    }
}
