# Migrations

This directory stores PostgreSQL schema migrations for the application database.

Use `sqlx migrate run` from the repository root. The repository-level `sqlx.toml`
points SQLx CLI at this directory and keeps `DATABASE_URL` as the connection
variable.
