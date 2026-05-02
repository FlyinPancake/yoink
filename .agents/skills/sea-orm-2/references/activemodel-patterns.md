# ActiveModel Patterns

## Table of Contents

- [Creating ActiveModels](#creating-activemodels)
- [Builder Pattern (2.0)](#builder-pattern-20)
- [Nested Persistence](#nested-persistence)
- [Updating Models](#updating-models)
- [Insert vs Save](#insert-vs-save)

## Creating ActiveModels

### From Default

```rust
let apple = fruit::ActiveModel {
    name: Set("Apple".to_owned()),
    ..Default::default()
};
```

### From Model

```rust
let model: fruit::Model = Fruit::find_by_id(1).one(db).await?.unwrap();
let active: fruit::ActiveModel = model.into();
```

### New with Required Fields

```rust
let apple = fruit::ActiveModel {
    id: NotSet,           // Auto-generated
    name: Set("Apple".to_owned()),
    cake_id: Set(None),   // Set explicitly
};
```

## Builder Pattern (2.0)

### Basic Creation

```rust
let user = user::ActiveModel::builder()
    .set_name("Bob")
    .set_email("bob@sea-ql.org")
    .insert(db)
    .await?;
```

### With Has-One Relation

```rust
let user = user::ActiveModel::builder()
    .set_name("Bob")
    .set_email("bob@sea-ql.org")
    .set_profile(profile::ActiveModel::builder().set_picture("avatar.png"))
    .save(db)
    .await?;
```

### With Has-Many Relation

```rust
let user = user::ActiveModel::builder()
    .set_name("Bob")
    .set_email("bob@sea-ql.org")
    .add_post(post::ActiveModel::builder().set_title("First post"))
    .add_post(post::ActiveModel::builder().set_title("Second post"))
    .save(db)
    .await?;
```

### With Many-to-Many

```rust
let post = post::ActiveModel::builder()
    .set_title("A sunny day")
    .set_user_id(user.id)
    .add_tag(existing_tag)  // Add existing tag model
    .add_tag(tag::ActiveModel::builder().set_tag("outdoor"))  // Create new tag
    .save(db)
    .await?;
```

## Nested Persistence

SeaORM 2.0 automatically handles the order of operations:

```rust
// Creates: user -> profile (1-1), posts (1-N), post -> tags (M-N)
let user = user::ActiveModel::builder()
    .set_name("Bob")
    .set_email("bob@sea-ql.org")
    .set_profile(profile::ActiveModel::builder().set_picture("image.jpg"))
    .add_post(
        post::ActiveModel::builder()
            .set_title("Nice weather")
            .add_tag(tag::ActiveModel::builder().set_tag("sunny"))
    )
    .save(db)
    .await?;
```

This generates:

1. INSERT user
2. INSERT profile (with user.id as FK)
3. INSERT post (with user.id as FK)
4. INSERT tag
5. INSERT post_tag junction rows

## Updating Models

### Partial Update

```rust
let pear: fruit::Model = Fruit::find_by_id(1).one(db).await?.unwrap();
let mut pear: fruit::ActiveModel = pear.into();

// Only changed fields will be updated
pear.name = Set("Sweet pear".to_owned());
let pear: fruit::Model = pear.update(db).await?;
```

### Using ActiveValue Methods

```rust
let mut pear: fruit::ActiveModel = /* ... */;

// Set value
pear.name = Set("Pear".to_owned());

// Mark as unchanged (won't be updated)
pear.name = Unchanged("Pear".to_owned());

// Reset to NotSet (will be excluded)
pear.cake_id = NotSet;

// Conditional set
if should_update {
    pear.name = Set("New name".to_owned());
}
```

### Bulk Update

```rust
Fruit::update_many()
    .col_expr(fruit::COLUMN.cake_id, fruit::COLUMN.cake_id.add(2))
    .filter(fruit::COLUMN.name.contains("Apple"))
    .exec(db)
    .await?;
```

## Insert vs Save

### Insert

- Always creates a new row
- Primary key should be `NotSet` (auto-generated) or set explicitly

```rust
let apple = fruit::ActiveModel {
    name: Set("Apple".to_owned()),
    ..Default::default()  // id is NotSet
};

let apple = apple.insert(db).await?;
```

### Save

- Creates if primary key is `NotSet`
- Updates if primary key is `Set` or `Unchanged`

```rust
let banana = fruit::ActiveModel {
    id: NotSet,
    name: Set("Banana".to_owned()),
    ..Default::default()
};

// Create
let mut banana = banana.save(db).await?;

banana.name = Set("Banana Mongo".to_owned());

// Update
let banana = banana.save(db).await?;
```

## ActiveValue States

```rust
use sea_orm::ActiveValue::{Set, NotSet, Unchanged};

// NotSet - field will be excluded from INSERT/UPDATE
let value: ActiveValue<String> = NotSet;

// Set - field has been modified, will be included in INSERT/UPDATE
let value: ActiveValue<String> = Set("value".to_owned());

// Unchanged - field loaded from database, won't be updated unless modified
let value: ActiveValue<String> = Unchanged("value".to_owned());
```

## Converting Between Types

```rust
// Model -> ActiveModel
let active: ActiveModel = model.into();

// ActiveModel -> Model (after insert/update)
let model: Model = active.insert(db).await?;

// ActiveModel -> Model (without DB operation, only if all fields are Set/Unchanged)
let model: Model = active.try_into().unwrap();
```
