#![feature(trait_alias)]
mod uri;

pub use uri::{Comparison, Order, UriBuilder};

use hyper::{
    body::Buf,
    client::{connect::Connect, Client},
    http::uri::{Authority, InvalidUri, Scheme},
    Body, Response, Uri,
};
use log::debug;
use serde::{de::DeserializeOwned, Deserialize};
use std::{convert::TryFrom, io::Read};
use thiserror::Error;

pub trait Connector = Connect + Clone + Send + Sync + 'static;

pub struct DataSource<C> {
    client: Client<C>,
    authority: Authority,
    base_path: String,
    scheme: Scheme,
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("invalid URI")]
    Uri(#[from] InvalidUri),
    #[error("http error")]
    Http(#[from] hyper::http::Error),
    #[error("hyper error")]
    Hyper(#[from] hyper::Error),
    #[error("serde error")]
    Serde(serde_json::Error, String),
    #[error("io error")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Deserialize)]
pub struct Page<T> {
    pub value: Vec<T>,
    #[serde(rename = "odata.count")]
    pub count: Option<String>,
    #[serde(rename = "odata.nextLink")]
    pub next_link: Option<String>,
}

pub async fn extract_as<T: DeserializeOwned>(response: Response<Body>) -> Result<T, Error> {
    let body = hyper::body::aggregate(response).await?;

    let mut content = String::new();
    body.reader().read_to_string(&mut content)?;

    serde_json::from_str(&content).map_err(|e| Error::Serde(e, content))
}

impl<C> DataSource<C>
where
    C: Connector,
{
    pub fn new<A>(client: Client<C>, domain: A, base_path: String) -> Result<DataSource<C>, Error>
    where
        Authority: TryFrom<A>,
        Error: From<<Authority as TryFrom<A>>::Error>,
    {
        Ok(DataSource {
            client,
            authority: Authority::try_from(domain)?,
            base_path,
            scheme: Scheme::HTTPS,
        })
    }

    pub async fn get<T>(&self, resource_type: &str, id: usize) -> Result<Response<Body>, Error> {
        let uri = Uri::builder()
            .scheme(self.scheme.as_ref())
            .authority(self.authority.as_ref())
            .path_and_query(
                UriBuilder::new_with_base(self.base_path.clone(), resource_type.to_string())
                    .id(id)
                    .build()?,
            )
            .build()
            .map_err(Error::from)?;

        debug!("fetching {}", uri);
        Ok(self.client.get(uri).await?)
    }
}
