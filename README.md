# odata-simple-client

This crate provides a Rust-interface to an [OData 3.0](https://www.odata.org/documentation/odata-version-3-0/) API over HTTP(S)

To get started, construct a `DataSource` and then create either a `ListRequest` or `GetRequest` and `fetch`/`fetch_paged` it using your `DataSource`

Here's a complete example which fetches a single `Dokument` from the [Danish Parliament's](https://oda.ft.dk) OData API:

```rust
use hyper::{Client, client::HttpConnector};
use hyper_openssl::{HttpsConnector};
use odata_simple_client::{DataSource, GetRequest};
use serde::Deserialize;

#[derive(Deserialize)]
struct Dokument {
    titel: String,
}

// Construct a Hyper client for communicating over HTTPS
let client: Client<HttpsConnector<HttpConnector>> =
    Client::builder().build(HttpsConnector::<HttpConnector>::new().unwrap());

// Set up our DataSource. The API is reachable on https://oda.ft.dk/api/
let datasource = DataSource::new(client, "oda.ft.dk", Some(String::from("/api"))).unwrap();

// The tokio_test::block_on call is just to make this example work in a rustdoc example.
// Normally you would just write the enclosed code in an async function.
tokio_test::block_on(async {
    let dokument: Dokument = datasource.fetch(
        GetRequest::new("Dokument", 24)
    ).await.unwrap();

    assert_eq!(dokument.titel, "Grund- og nærhedsnotat vedr. sanktioner på toldområdet");
});
```

The example above has requirements on a number of crates. See the `Cargo.toml`-file for a list.