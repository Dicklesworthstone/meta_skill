# async-object-pool
A simple object pool implementation that uses asynchronous synchronization primitives only. 

You can use it as follows:

```rust
use async_object_pool::Pool;

// Create a new pool that will allow to create at most 100 items
let pool = Pool::new(100);

// Take an item from the pool or create a new item if the pool is empty
// but the maximum number of pooled items was not created yet.
// This will asynchronously block execution until an item can be returned.
let item = pool.take_or_create(|| String::from("hello")).await;

// Use your item
println!("{}", item);

// After using the item, put it back into the pool so it can be reused elsewhere
pool.put(item).await;
```

This crate is used to pool HTTP mock servers in [httpmock](https://github.com/alexliesenfeld/httpmock).

## License
`async-object-pool` is free software: you can redistribute it and/or modify it under the terms of the MIT Public License.
 
This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied 
warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the MIT Public License for more details.
