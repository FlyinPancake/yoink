# Query Patterns

## Table of Contents

- [Basic Queries](#basic-queries)
- [Filtering](#filtering)
- [Entity Loader API](#entity-loader-api)
- [Joins](#joins)
- [Pagination](#pagination)
- [Aggregation](#aggregation)
- [Raw SQL](#raw-sql)

## Basic Queries

### Find All

```rust
let cakes: Vec<cake::Model> = Cake::find().all(db).await?;
```

### Find One

```rust
let cheese: Option<cake::Model> = Cake::find_by_id(1).one(db).await?;
let cheese: cake::Model = Cheese::find_by_id(1).one(db).await?.unwrap();
```

### Find or Create

```rust
let cake = match Cake::find_by_id(1).one(db).await? {
    Some(cake) => cake,
    None => cake::ActiveModel {
        name: Set("New Cake".to_owned()),
        ..Default::default()
    }.insert(db).await?,
};
```

## Filtering

### Using Strongly-Typed Columns

```rust
// 2.0 preferred syntax
let chocolate: Vec<cake::Model> = Cake::find()
    .filter(Cake::COLUMN.name.contains("chocolate"))
    .all(db)
    .await?;

// Compound conditions
let cakes = Cake::find()
    .filter(
        Condition::all()
            .add(Cake::COLUMN.name.contains("chocolate"))
            .add(Cake::COLUMN.price.gt(10))
    )
    .all(db)
    .await?;
```

### Comparison Operators

```rust
// Equality
.filter(user::COLUMN.id.eq(42))
.filter(user::COLUMN.name.ne("Bob"))

// Comparison
.filter(cake::COLUMN.price.gt(10))
.filter(cake::COLUMN.price.gte(10))
.filter(cake::COLUMN.price.lt(100))
.filter(cake::COLUMN.price.lte(100))

// Null checks
.filter(user::COLUMN.deleted_at.is_null())
.filter(user::COLUMN.deleted_at.is_not_null())

// Range
.filter(cake::COLUMN.price.between(10, 100))

// In
.filter(cake::COLUMN.id.is_in([1, 2, 3]))
.filter(cake::COLUMN.id.is_not_in([4, 5, 6]))
```

### String Matching

```rust
// Contains (LIKE %value%)
.filter(user::COLUMN.name.contains("bob"))

// Starts with (LIKE value%)
.filter(user::COLUMN.email.starts_with("admin"))

// Ends with (LIKE %value)
.filter(user::COLUMN.email.ends_with("@example.com"))

// Exact match with wildcards
.filter(user::COLUMN.name.like("B_b"))
```

### Conditions

```rust
// AND
.filter(
    Condition::all()
        .add(cake::COLUMN.name.contains("chocolate"))
        .add(cake::COLUMN.price.gt(10))
)

// OR
.filter(
    Condition::any()
        .add(cake::COLUMN.name.contains("chocolate"))
        .add(cake::COLUMN.name.contains("vanilla"))
)

// Nested
.filter(
    Condition::all()
        .add(cake::COLUMN.price.gt(10))
        .add(
            Condition::any()
                .add(cake::COLUMN.name.contains("chocolate"))
                .add(cake::COLUMN.name.contains("vanilla"))
        )
)
```

## Entity Loader API

The `Entity::load()` API (2.0) handles eager loading efficiently.

### Basic Loading

```rust
let user = user::Entity::load()
    .filter_by_id(42)
    .with(profile::Entity)  // Load 1-1 relation
    .with(post::Entity)     // Load 1-N relation
    .one(db)
    .await?;
```

### Nested Loading

```rust
// user -> posts -> comments
let user = user::Entity::load()
    .filter_by_id(42)
    .with((post::Entity, comment::Entity))
    .one(db)
    .await?;
```

### Multiple Relations

```rust
let user = user::Entity::load()
    .filter_by_id(42)
    .with(profile::Entity)
    .with(post::Entity)
    .with(comment::Entity)
    .one(db)
    .await?;
```

### Many-to-Many

```rust
// post -> tags via post_tag junction
let post = post::Entity::load()
    .filter_by_id(1)
    .with(tag::Entity)
    .one(db)
    .await?;
```

## Joins

### Traditional Join Methods

```rust
// Inner join
Cake::find()
    .inner_join(fruit::Entity)
    .all(db)
    .await?;

// Left join
Cake::find()
    .left_join(fruit::Entity)
    .all(db)
    .await?;

// Right join
Cake::find()
    .right_join(fruit::Entity)
    .all(db)
    .await?;
```

### Find with Related

```rust
// 1-1: Returns (Model, Option<Related>)
let cake_with_fruit: Vec<(cake::Model, Option<fruit::Model>)> =
    Cake::find().find_also_related(Fruit).all(db).await?;

// 1-N: Returns (Model, Vec<Related>)
let cake_with_fillings: Vec<(cake::Model, Vec<filling::Model>)> =
    Cake::find().find_with_related(Filling).all(db).await?;
```

### Custom Join Conditions

```rust
Cake::find()
    .join(
        JoinType::LeftJoin,
        fruit::Entity,
        Expr::col((cake::Entity, cake::Column::Id))
            .equals((fruit::Entity, fruit::Column::CakeId))
    )
    .all(db)
    .await?;
```

## Pagination

### Basic Pagination

```rust
use sea_orm::{EntityTrait, QuerySelect};

let pages = Cake::find()
    .paginate(db, 10);  // 10 items per page

while let Some(cakes) = pages.fetch_and_next().await? {
    // Process cakes
}
```

### Get Page

```rust
let paginator = Cake::find().paginate(db, 10);
let page = paginator.fetch_page(2).await?;  // Third page (0-indexed)
```

### Total Items

```rust
let paginator = Cake::find().paginate(db, 10);
let total_items = paginator.num_items().await?;
let total_pages = paginator.num_pages().await?;
```

## Aggregation

### Count

```rust
use sea_orm::{EntityTrait, QuerySelect};

let count = Cake::find()
    .filter(Cake::COLUMN.name.contains("chocolate"))
    .count(db)
    .await?;
```

### Group By

```rust
use sea_orm::{FromQueryResult, QuerySelect};

#[derive(Debug, PartialEq, FromQueryResult)]
struct CountResult {
    name: String,
    count: i64,
}

let results = cake::Entity::find()
    .select_only()
    .column(cake::Column::Name)
    .column_as(cake::Column::Id.count(), "count")
    .group_by(cake::Column::Name)
    .into_model::<CountResult>()
    .all(db)
    .await?;
```

## Raw SQL

### Raw SQL with Parameter Binding

```rust
use sea_orm::raw_sql;

let ids = [2, 3, 4];

let user: Option<user::Model> = user::Entity::find()
    .from_raw_sql(raw_sql!(
        Sqlite,
        r#"SELECT "id", "name" FROM "user"
           WHERE "id" in ({..ids})
        "#
    ))
    .one(db)
    .await?;
```

### Custom Result Types

```rust
#[derive(FromQueryResult)]
struct CakeWithBakery {
    name: String,
    #[sea_orm(nested)]
    bakery: Option<Bakery>,
}

let cake: Option<CakeWithBakery> = CakeWithBakery::find_by_statement(raw_sql!(
    Sqlite,
    r#"SELECT "cake"."name", "bakery"."name" AS "bakery_name"
       FROM "cake"
       LEFT JOIN "bakery" ON "cake"."bakery_id" = "bakery"."id"
       WHERE "cake"."id" = 1"#
))
.one(db)
.await?;
```

## Partial Models

Select only the columns you need:

```rust
use sea_orm::DerivePartialModel;

#[derive(DerivePartialModel)]
#[sea_orm(entity = "cake::Entity")]
struct CakeName {
    id: i32,
    name: String,
}

let cakes: Vec<CakeName> = Cake::find()
    .into_partial_model()
    .all(db)
    .await?;
```

### Nested Partial Models

```rust
#[derive(DerivePartialModel)]
#[sea_orm(entity = "cake::Entity")]
struct CakeWithFruit {
    id: i32,
    name: String,
    #[sea_orm(nested)]
    fruit: Option<fruit::Model>,
}

let cakes: Vec<CakeWithFruit> = Cake::find()
    .left_join(fruit::Entity)
    .into_partial_model()
    .all(db)
    .await?;
```
