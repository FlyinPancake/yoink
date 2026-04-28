---
name: sea-orm-2
description: Expert guidance for SeaORM 2.0, Rust's async ORM with strongly-typed columns, nested ActiveModels, Entity Loader API, and entity-first workflow. Use when working with SeaORM entities, queries, migrations, relations, or any database operations in Rust projects using SeaORM. Covers the new 2.0 API patterns, anti-patterns to avoid, and migration from 1.0.
---

# SeaORM 2.0

This skill provides expert guidance for using SeaORM 2.0 properly, focusing on the new patterns and avoiding common pitfalls that stem from SeaORM 1.0 knowledge.

## Quick Reference

- [Walk-through of SeaORM 2.0](https://www.sea-ql.org/blog/2025-12-05-sea-orm-2.0/)
- [Migration Guide (1.0 to 2.0)](https://www.sea-ql.org/blog/2026-01-12-sea-orm-2.0/)
- [New Entity Format](https://www.sea-ql.org/blog/2025-10-20-sea-orm-2.0/)
- [Strongly-Typed Column](https://www.sea-ql.org/blog/2025-11-11-sea-orm-2.0/)
- [Nested ActiveModel](https://www.sea-ql.org/blog/2025-11-25-sea-orm-2.0/)

## Entity Definition (2.0 Format)

SeaORM 2.0 uses `#[sea_orm::model]` with relations defined directly on the `Model` struct. This replaces the 1.0 pattern of separate `Relation` enums and `Related` trait impls.

### Basic Entity

```rust
mod user {
    use sea_orm::entity::prelude::*;

    #[sea_orm::model]
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "user")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        pub name: String,
        #[sea_orm(unique)]
        pub email: String,
    }

    impl ActiveModelBehavior for ActiveModel {}
}
```

### Entity with Relations

```rust
mod user {
    use sea_orm::entity::prelude::*;

    #[sea_orm::model]
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "user")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        pub name: String,
        #[sea_orm(unique)]
        pub email: String,
        #[sea_orm(has_one)]
        pub profile: HasOne<super::profile::Entity>,
        #[sea_orm(has_many)]
        pub posts: HasMany<super::post::Entity>,
    }
}
```

### Relation Types

```rust
// Has-One
#[sea_orm(has_one)]
pub profile: HasOne<super::profile::Entity>,

// Has-Many
#[sea_orm(has_many)]
pub posts: HasMany<super::post::Entity>,

// Belongs-To (explicit foreign key mapping)
#[sea_orm(belongs_to, from = "user_id", to = "id")]
pub user: HasOne<super::user::Entity>,

// Many-to-Many via junction table
#[sea_orm(has_many, via = "post_tag")]
pub tags: HasMany<super::tag::Entity>,

// Self-referential
#[sea_orm(self_ref, via = "user_follower", from = "User", to = "Follower")]
pub followers: HasMany<Entity>,
```

### Junction Table (Composite Primary Key)

```rust
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "post_tag")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub post_id: i32,
    #[sea_orm(primary_key, auto_increment = false)]
    pub tag_id: i32,
    #[sea_orm(belongs_to, from = "post_id", to = "id")]
    pub post: Option<super::post::Entity>,
    #[sea_orm(belongs_to, from = "tag_id", to = "id")]
    pub tag: Option<super::tag::Entity>,
}
```

For detailed entity patterns, see [references/entity-patterns.md](references/entity-patterns.md).

## Strongly-Typed Columns (2.0)

Use `COLUMN` constant with typed fields instead of the untyped `Column` enum:

```rust
// 2.0 (preferred) -- compile-time type safety
user::Entity::find().filter(user::COLUMN.name.contains("Bob"))

// 1.0 (outdated) -- still works but prefer COLUMN
user::Entity::find().filter(user::Column::Name.contains("Bob"))
```

The `COLUMN` constant provides:

- Compile-time type checking
- Better IDE autocomplete
- Refactoring safety

## ActiveModel Builder Pattern (2.0)

### Creating with Nested Relations

```rust
// Create with nested relations
let bob = user::ActiveModel::builder()
    .set_name("Bob")
    .set_email("bob@sea-ql.org")
    .set_profile(profile::ActiveModel::builder().set_picture("Tennis"))
    .insert(db)
    .await?;
```

### Adding Has-Many Children

```rust
let mut bob = bob.into_active_model();
bob.posts.push(
    post::ActiveModel::builder().set_title("My first post")
);
bob.save(db).await?;
```

### Many-to-Many Relations

```rust
let post = post::ActiveModel::builder()
    .set_title("A sunny day")
    .set_user_id(bob.id)
    .add_tag(existing_tag)
    .add_tag(tag::ActiveModel::builder().set_tag("outdoor"))
    .save(db)
    .await?;
```

For complete ActiveModel patterns, see [references/activemodel-patterns.md](references/activemodel-patterns.md).

## Entity Loader API (2.0)

The Entity Loader eliminates N+1 queries by intelligently using JOIN for 1-1 relations and data loader for 1-N relations.

### Basic Loading

```rust
// Load with relations in a single query
let bob = user::Entity::load()
    .filter_by_email("bob@sea-ql.org")
    .with(profile::Entity)
    .with(post::Entity)
    .one(db)
    .await?
    .expect("Not found");
```

### Nested Relations

```rust
// Nested relations (post -> comments)
let user = user::Entity::load()
    .filter_by_id(12)
    .with(profile::Entity)
    .with((post::Entity, comment::Entity))
    .one(db)
    .await?;
```

### How It Works

```rust
// join paths:
// user -> profile
// user -> post
//         post -> post_tag -> tag
let smart_user = user::Entity::load()
    .filter_by_id(42)
    .with(profile::Entity) // 1-1 uses join
    .with((post::Entity, tag::Entity)) // 1-N uses data loader
    .one(db)
    .await?
    .unwrap();

// 3 queries are executed under the hood:
// 1. SELECT FROM user JOIN profile WHERE id = $
// 2. SELECT FROM post WHERE user_id IN (..)
// 3. SELECT FROM tag JOIN post_tag WHERE post_id IN (..)
```

For advanced querying patterns, see [references/query-patterns.md](references/query-patterns.md).

## Schema Registry (Entity-First Workflow)

SeaORM 2.0 supports entity-first development where you define entities and SeaORM creates the schema.

```rust
// Auto-create tables from entity definitions (dev/testing)
db.get_schema_registry("my_crate::*")
    .sync(db)
    .await?;
```

This:

- Creates tables in topological order based on foreign key dependencies
- Adds new columns to existing tables
- Creates unique keys and foreign keys automatically

Requires feature flags: `entity-registry` and `schema-sync`.

## Anti-Patterns to Avoid

### 1. Do Not Specify `column_type` on Custom Wrapper Types

```rust
// WRONG -- do not annotate column_type on custom types
#[sea_orm(column_type = "Decimal(Some((10, 4)))")]
pub speed: Speed,

// CORRECT -- SeaORM infers the column type from DeriveValueType
pub speed: Speed,

#[derive(Clone, Debug, PartialEq, DeriveValueType)]
pub struct Speed(Decimal);
```

### 2. Use `Text` for Long Strings on MySQL/MSSQL

```rust
// WRONG on MySQL/MSSQL -- silently truncates at 255 chars
pub description: String,

// CORRECT -- use column_type for longer strings
#[sea_orm(column_type = "Text")]
pub description: String,

// Also correct -- explicit max length
#[sea_orm(column_type = "String(StringLen::Max)")]
pub event_type: String,
```

### 3. Missing `ExprTrait` Import

Methods like `.eq()`, `.like()`, `.contains()` on `Expr` require the trait import in 2.0:

```rust
use sea_orm::ExprTrait; // required in 2.0

Expr::col((self.entity_name(), *self)).like(s)
```

### 4. Do Not Use Removed or Renamed APIs

| 1.0 (removed/renamed)                            | 2.0 (correct)                                        |
| ------------------------------------------------ | ---------------------------------------------------- |
| `.into_condition()`                              | `.into()`                                            |
| `db.execute(Statement::from_sql_and_values(..))` | `db.execute_raw(Statement::from_sql_and_values(..))` |
| `db.query_all(backend.build(&query))`            | `db.query_all(&query)`                               |
| `Alias::new("col")` for static strings           | `Expr::col("col")` directly                          |
| `insert_many(..).on_empty_do_nothing()`          | `insert_many([])` returns `None` safely              |

### 5. Do Not Manually impl Traits That `DeriveValueType` Generates

In 2.0, `DeriveValueType` auto-generates `NotU8`, `IntoActiveValue`, and `TryFromU64`. Remove manual implementations to avoid conflicts.

### 6. PostgreSQL: `serial` is No Longer Default

Auto-increment columns now use `GENERATED BY DEFAULT AS IDENTITY`. Use feature flag `postgres-use-serial-pk` for legacy behavior.

### 7. SQLite: Integer Type Mapping Changed

Both `Integer` and `BigInteger` map to `integer` in 2.0. Use `sea-orm-cli --big-integer-type=i32` if needed.

For complete migration guide, see [references/migration-guide.md](references/migration-guide.md).

## Common Operations Quick Reference

### Select

```rust
// Find all
let cakes: Vec<cake::Model> = Cake::find().all(db).await?;

// Filter
let chocolate: Vec<cake::Model> = Cake::find()
    .filter(Cake::COLUMN.name.contains("chocolate"))
    .all(db)
    .await?;

// Find by ID
let cheese: Option<cake::Model> = Cake::find_by_id(1).one(db).await?;

// Eager load related (1-1)
let cake_with_fruit: Vec<(cake::Model, Option<fruit::Model>)> =
    Cake::find().find_also_related(Fruit).all(db).await?;

// Eager load related (1-N and M-N)
let cake_with_fillings: Vec<(cake::Model, Vec<filling::Model>)> = Cake::find()
    .find_with_related(Filling)
    .all(db)
    .await?;
```

### Insert

```rust
let apple = fruit::ActiveModel {
    name: Set("Apple".to_owned()),
    ..Default::default()
};

// Insert one
let apple = apple.insert(db).await?;

// Insert many
let result = Fruit::insert_many([apple, pear]).exec(db).await?;
```

### Update

```rust
let pear: Option<fruit::Model> = Fruit::find_by_id(1).one(db).await?;
let mut pear: fruit::ActiveModel = pear.unwrap().into();

pear.name = Set("Sweet pear".to_owned());
let pear: fruit::Model = pear.update(db).await?;

// Bulk update
Fruit::update_many()
    .col_expr(fruit::COLUMN.cake_id, fruit::COLUMN.cake_id.add(2))
    .filter(fruit::COLUMN.name.contains("Apple"))
    .exec(db)
    .await?;
```

### Delete

```rust
// Delete one
orange.delete(db).await?;

// Delete many
fruit::Entity::delete_many()
    .filter(fruit::COLUMN.name.contains("Orange"))
    .exec(db)
    .await?;
```

## Feature Flags

Key feature flags to know:

- `sqlx-mysql`, `sqlx-postgres`, `sqlx-sqlite` - Database drivers
- `runtime-tokio-native-tls`, `runtime-tokio-rustls` - Async runtime
- `with-chrono`, `with-time` - Date/time libraries
- `with-rust_decimal`, `with-bigdecimal` - Decimal libraries
- `with-json` - JSON support
- `entity-registry`, `schema-sync` - Entity-first workflow
- `postgres-use-serial-pk` - Legacy PostgreSQL serial types

---

### references/entity-patterns.md

- Basic entity structure
- All relation types (Has-One, Has-Many, Many-to-Many, Self-referential)
- Composite primary keys
- Custom types with DeriveValueType
- Column attributes reference

### references/activemodel-patterns.md

- Creating ActiveModels (3 methods)
- Builder pattern usage
- Nested persistence examples
- Update patterns
- Insert vs Save semantics
- ActiveValue states

### references/query-patterns.md

- Basic CRUD queries
- Filtering with strongly-typed columns
- Entity Loader API patterns
- Join operations
- Pagination
- Aggregation
- Raw SQL with parameter binding

### references/migration-guide.md

- Complete 1.0 to 2.0 migration guide
- Breaking changes table
- API renames reference
- Step-by-step migration instructions

This skill provides comprehensive coverage of SeaORM 2.0 with focus on the new patterns and critical anti-patterns to avoid.
