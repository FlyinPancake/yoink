# Entity Patterns

## Table of Contents

- [Basic Entity](#basic-entity)
- [Entity with Relations](#entity-with-relations)
- [Composite Primary Keys](#composite-primary-keys)
- [Custom Types](#custom-types)
- [Column Attributes](#column-attributes)

## Basic Entity

The minimal entity structure in SeaORM 2.0:

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
        pub email: String,
    }

    impl ActiveModelBehavior for ActiveModel {}
}
```

## Entity with Relations

### Has-One Relation

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
        #[sea_orm(has_one)]
        pub profile: HasOne<super::profile::Entity>,
    }
}

mod profile {
    use sea_orm::entity::prelude::*;

    #[sea_orm::model]
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "profile")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        pub user_id: i32,
        pub picture: String,
        #[sea_orm(belongs_to, from = "user_id", to = "id")]
        pub user: HasOne<super::user::Entity>,
    }
}
```

### Has-Many Relation

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
        #[sea_orm(has_many)]
        pub posts: HasMany<super::post::Entity>,
    }
}

mod post {
    use sea_orm::entity::prelude::*;

    #[sea_orm::model]
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "post")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        pub user_id: i32,
        pub title: String,
        #[sea_orm(belongs_to, from = "user_id", to = "id")]
        pub author: HasOne<super::user::Entity>,
    }
}
```

### Many-to-Many Relation

```rust
mod post {
    use sea_orm::entity::prelude::*;

    #[sea_orm::model]
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "post")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        pub title: String,
        #[sea_orm(has_many, via = "post_tag")]
        pub tags: HasMany<super::tag::Entity>,
    }
}

mod tag {
    use sea_orm::entity::prelude::*;

    #[sea_orm::model]
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "tag")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        pub tag: String,
    }
}

mod post_tag {
    use sea_orm::entity::prelude::*;

    #[sea_orm::model]
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "post_tag")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub post_id: i32,
        #[sea_orm(primary_key, auto_increment = false)]
        pub tag_id: i32,
    }
}
```

### Self-Referential Relation

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
        #[sea_orm(self_ref, via = "user_follower", from = "User", to = "Follower")]
        pub followers: HasMany<Entity>,
    }
}
```

## Composite Primary Keys

```rust
mod post_tag {
    use sea_orm::entity::prelude::*;

    #[sea_orm::model]
    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
    #[sea_orm(table_name = "post_tag")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub post_id: i32,
        #[sea_orm(primary_key, auto_increment = false)]
        pub tag_id: i32,
    }
}
```

## Custom Types

### Using DeriveValueType

```rust
#[derive(Clone, Debug, PartialEq, DeriveValueType)]
pub struct Speed(Decimal);

// In entity
pub struct Model {
    pub speed: Speed, // Column type inferred automatically
}
```

### String Newtypes

```rust
#[derive(Clone, Debug, PartialEq, DeriveValueType)]
pub struct Email(String);

// SeaORM infers VARCHAR(255) for inner String type
```

## Column Attributes

### Primary Key

```rust
#[sea_orm(primary_key)]
pub id: i32,

#[sea_orm(primary_key, auto_increment = false)]
pub custom_id: String,
```

### Unique Constraint

```rust
#[sea_orm(unique)]
pub email: String,
```

### Nullable

```rust
pub middle_name: Option<String>,
```

### Column Type Override

```rust
#[sea_orm(column_type = "Text")]
pub description: String,

#[sea_orm(column_type = "Decimal(Some((10, 4)))")]
pub price: Decimal,

#[sea_orm(column_type = "String(StringLen::Max)")]
pub long_text: String,
```

### Index

```rust
#[sea_orm(index)]
pub category: String,
```

### Default Value

```rust
#[sea_orm(default_value = "active")]
pub status: String,
```

## Generated Code

When using `#[sea_orm::model]`, SeaORM generates:

1. `Entity` - unit struct for type-level operations
2. `Model` - data struct with your fields
3. `ActiveModel` - mutable struct with `ActiveValue<T>` fields
4. `Column` - enum for column references (untyped)
5. `COLUMN` - constant with typed column references (2.0+)
6. `PrimaryKey` - enum for primary key columns
7. Relation traits based on attributes
