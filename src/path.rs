use std::{collections::HashMap, convert::TryInto};

use hyper::http::uri::{InvalidUri, PathAndQuery};

/// Specifies direction in which the returned results are listed. Use [`ListRequest::order_by`](`crate::ListRequest::order_by`) to change it.
/// If nothing else is specified, it defaults to [`Direction::Ascending`]
#[derive(Debug, Clone, Copy)]
pub enum Direction {
    /// List results in descending order (largest to smallest)
    Descending,
    /// List results in ascending order (smallest to largest)
    Ascending,
}

/// Used by [`ListRequest::filter`](`crate::ListRequest::filter`) to apply conditional filtering to the returned results.
///
/// See [the OData 3.0 documentation (section 5.1.2)](https://www.odata.org/documentation/odata-version-3-0/url-conventions/) for more information.
#[derive(Debug, Clone, Copy)]
pub enum Comparison {
    /// The Equal operator evaluates to true if the field is equal to the value, otherwise if evaluates to false.
    Equal,
    /// The NotEqual operator evaluates to true if the field is not equal to the value, otherwise if evaluates to false.
    NotEqual,
    /// The GreaterThan operator evaluates to true if the field is greater than the value, otherwise if evaluates to false.
    GreaterThan,
    /// The GreaterOrEqual operator  evaluates to true if the field is greater than or equal to the value, otherwise if evaluates to false.
    GreaterOrEqual,
    /// The LessThan operator evaluates to true if the field is less than the value, otherwise if evaluates to false.
    LessThan,
    /// LessOrEqual operator evaluates to true if the field is less than or equal to the value, otherwise if evaluates to false.
    LessOrEqual,
}

/// Format of the returned API data. [`DataSource::fetch_paged`](`crate::DataSource::fetch_paged`) forces [`Format::Json`].
#[derive(Debug, Clone, Copy)]
pub enum Format {
    /// Request that the returned API data is xml-formatted.
    Xml,
    /// Request that the returned API data is json-formatted.
    Json,
}

/// Used by [`ListRequest::inline_count`](`crate::ListRequest::inline_count`) to show number of results left in a query, before all pages have been read.
#[derive(Debug, Clone, Copy)]
pub enum InlineCount {
    /// Don't include an inline count.
    None,
    /// Include inline count on all pages.
    AllPages,
}

#[derive(Debug, Clone)]
pub(crate) struct PathBuilder {
    pub(crate) base_path: String,
    resource_type: String,
    id: Option<usize>,
    inner: HashMap<&'static str, String>,
}

impl PathBuilder {
    pub fn new_with_base(base_path: String, resource_type: String) -> Self {
        PathBuilder {
            id: None,
            base_path,
            resource_type,
            inner: HashMap::new(),
        }
    }

    pub fn new(resource_type: String) -> Self {
        Self::new_with_base(String::new(), resource_type)
    }

    pub fn id(mut self, id: usize) -> Self {
        self.id = Some(id);
        self
    }

    pub fn base_path(mut self, base_path: String) -> Self {
        self.base_path = base_path;
        self
    }

    pub fn order_by(mut self, field: &str, order: Direction) -> Self {
        let order = match order {
            Direction::Descending => "desc",
            Direction::Ascending => "asc",
        };

        // We don't really care if the value is overwritten.
        let _ = self.inner.insert(
            "orderby",
            urlencoding::encode(&format!("{field} {order}")).to_string(),
        );
        self
    }

    pub fn top(mut self, count: u32) -> Self {
        // We don't really care if the value is overwritten.
        let _ = self
            .inner
            .insert("top", urlencoding::encode(&count.to_string()).to_string());
        self
    }

    pub fn format(mut self, format: Format) -> Self {
        // We don't really care if the value is overwritten.
        let _ = self.inner.insert(
            "format",
            match format {
                Format::Xml => "xml",
                Format::Json => "json",
            }
            .to_string(),
        );
        self
    }

    pub fn skip(mut self, count: u32) -> Self {
        // We don't really care if the value is overwritten.
        let _ = self
            .inner
            .insert("skip", urlencoding::encode(&count.to_string()).to_string());
        self
    }

    pub fn inline_count(mut self, value: InlineCount) -> Self {
        // We don't really care if the value is overwritten.
        let _ = self.inner.insert(
            "inlinecount",
            urlencoding::encode(match value {
                InlineCount::None => "none",
                InlineCount::AllPages => "allpages",
            })
            .to_string(),
        );
        self
    }

    pub fn filter(mut self, field: &str, comparison: Comparison, value: &str) -> Self {
        let comparison = match comparison {
            Comparison::Equal => "eq",
            Comparison::NotEqual => "ne",
            Comparison::GreaterThan => "gt",
            Comparison::GreaterOrEqual => "ge",
            Comparison::LessThan => "lt",
            Comparison::LessOrEqual => "le",
        };

        // We don't really care if the value is overwritten.
        let _ = self.inner.insert(
            "filter",
            urlencoding::encode(&format!("{field} {comparison} {value}")).to_string(),
        );
        self
    }

    pub fn expand<'f, F>(mut self, field: F) -> Self
    where
        F: IntoIterator<Item = &'f str>,
    {
        let encoded = field
            .into_iter()
            .map(|field| urlencoding::encode(field).into_owned())
            .collect::<Vec<_>>()
            .join(",");

        // We don't really care if the value is overwritten.
        let _ = self
            .inner
            .entry("expand")
            .and_modify(|current| {
                current.push(',');
                current.push_str(&encoded)
            })
            .or_insert_with(|| encoded.to_string());
        self
    }

    pub fn build(&self) -> Result<PathAndQuery, InvalidUri> {
        let query = {
            let mut kv = self
                .inner
                .iter()
                .map(|(key, value)| {
                    format!(
                        "${key}={value}",
                        key = urlencoding::encode(key),
                        value = value
                    )
                })
                .collect::<Vec<_>>();
            kv.sort();
            kv
        };

        format!(
            "{base_path}/{resource_type}{id}?{query}",
            base_path = self.base_path,
            resource_type = urlencoding::encode(&self.resource_type),
            id = self
                .id
                .map(|id| format!("({})", urlencoding::encode(&id.to_string())))
                .unwrap_or_default(),
            query = query.join("&")
        )
        .parse()
    }
}

impl TryInto<PathAndQuery> for PathBuilder {
    type Error = InvalidUri;

    fn try_into(self) -> Result<PathAndQuery, Self::Error> {
        self.build()
    }
}

#[cfg(test)]
mod tests {
    use super::PathBuilder;
    use crate::Direction;

    #[test]
    fn test_query_builder() {
        let query = PathBuilder::new("test_resource".into())
            .top(2)
            .skip(3)
            .order_by("date", Direction::Ascending)
            .build()
            .unwrap();

        assert_eq!("/test_resource?$orderby=date%20asc&$skip=3&$top=2", query);
    }

    #[test]
    fn test_single_resource_expand() {
        let query = PathBuilder::new("test_resource".into())
            .id(100)
            .expand(["DoThing", "What"])
            .expand(["Hello"])
            .build()
            .unwrap();

        assert_eq!("/test_resource(100)?$expand=DoThing,What,Hello", query);
    }
}
