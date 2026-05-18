use serde_json::{Map, Value};
use sqlx::types::Json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::{Node, Properties};

#[derive(Clone, Debug)]
pub struct NodeRepository {
    pool: PgPool,
}

impl NodeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, node: &Node) -> Result<Node, sqlx::Error> {
        sqlx::query_as::<_, Node>(
            r#"
            INSERT INTO nodes (id, labels, properties)
            VALUES ($1, $2, $3)
            RETURNING id, labels, properties
            "#,
        )
        .bind(node.id)
        .bind(&node.labels)
        .bind(Json(&node.properties))
        .fetch_one(&self.pool)
        .await
    }

    pub async fn get_by_id(&self, node_id: Uuid) -> Result<Option<Node>, sqlx::Error> {
        sqlx::query_as::<_, Node>(
            r#"
            SELECT id, labels, properties
            FROM nodes
            WHERE id = $1
            "#,
        )
        .bind(node_id)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn scan_by_label(&self, label: &str, limit: i64) -> Result<Vec<Node>, sqlx::Error> {
        sqlx::query_as::<_, Node>(
            r#"
            SELECT id, labels, properties
            FROM nodes
            WHERE labels @> ARRAY[$1]::TEXT[]
            ORDER BY created_at ASC, id ASC
            LIMIT $2
            "#,
        )
        .bind(label)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn filter_by_property(
        &self,
        key: &str,
        value: Value,
        limit: i64,
    ) -> Result<Vec<Node>, sqlx::Error> {
        let property_filter = property_filter_value(key, value);

        sqlx::query_as::<_, Node>(
            r#"
            SELECT id, labels, properties
            FROM nodes
            WHERE properties @> $1
            ORDER BY created_at ASC, id ASC
            LIMIT $2
            "#,
        )
        .bind(Json(property_filter))
        .bind(limit)
        .fetch_all(&self.pool)
        .await
    }
}

fn property_filter_value(key: &str, value: Value) -> Properties {
    let mut properties = Map::with_capacity(1);
    properties.insert(key.to_owned(), value);
    properties
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::property_filter_value;

    #[test]
    fn property_filter_value_wraps_key_and_value_for_jsonb_contains() {
        let filter = property_filter_value("name", Value::String("alice".to_owned()));

        assert_eq!(json!(filter), json!({ "name": "alice" }));
    }

    #[test]
    fn property_filter_value_preserves_scalar_types() {
        let filter = property_filter_value("active", Value::Bool(true));

        assert_eq!(json!(filter), json!({ "active": true }));
    }
}
