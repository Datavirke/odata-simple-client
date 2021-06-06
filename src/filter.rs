use std::collections::HashMap;

use itertools::Itertools;

pub enum Order {
    Descending,
    Ascending,
}

pub enum Comparison {
    Equal,
    NotEqual,
    GreaterThan,
    GreaterOrEqual,
    LessThan,
    LessOrEqual,
}

#[derive(Default)]
pub struct Filter {
    inner: HashMap<&'static str, String>,
}

impl Filter {
    pub fn order_by(mut self, field: &str, order: Option<Order>) -> Self {
        let order = match order.unwrap_or(Order::Ascending) {
            Order::Descending => "desc",
            Order::Ascending => "asc",
        };

        self.inner.insert(
            "orderby",
            format!("{field} {order}", field = field, order = order),
        );
        self
    }

    pub fn top(mut self, count: u32) -> Self {
        self.inner.insert("top", count.to_string());
        self
    }

    pub fn skip(mut self, count: u32) -> Self {
        self.inner.insert("skip", count.to_string());
        self
    }

    pub fn inline_count(mut self, value: String) -> Self {
        self.inner.insert("inlinecount", value);
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

        self.inner.insert(
            "filter",
            format!(
                "{field} {comparison} {value}",
                field = field,
                comparison = comparison,
                value = value
            ),
        );
        self
    }

    pub fn to_query(&self) -> String {
        self.inner
            .iter()
            .sorted()
            .map(|(key, value)| {
                format!(
                    "${key}={value}",
                    key = urlencoding::encode(key),
                    value = urlencoding::encode(value)
                )
            })
            .join("&")
    }
}

#[cfg(test)]
mod tests {
    use super::Filter;

    #[test]
    fn test_query_builder() {
        let query = Filter::default()
            .top(2)
            .skip(3)
            .order_by("date", None)
            .to_query();

        assert_eq!("$orderby=date%20asc&$skip=3&$top=2", query);
    }
}
