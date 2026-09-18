//! Bounded Blockscout v2 pagination, shared by transaction and token history.
use crate::errors::EvmError;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct Page {
    pub items: Vec<Value>,
    pub next_page_params: Option<Value>,
}

pub async fn pages(
    http: &reqwest::Client,
    url: &str,
    query: &[(&str, &str)],
    max_pages: u32,
) -> Result<(Page, u32), EvmError> {
    if !(1..=100).contains(&max_pages) {
        return Err(EvmError::validation("--pages must be between 1 and 100"));
    }
    let mut all = Page {
        items: Vec::new(),
        next_page_params: None,
    };
    let mut seen = std::collections::HashSet::new();
    for page_index in 1..=max_pages {
        let mut request = http.get(url).query(query);
        if let Some(cursor) = &all.next_page_params {
            let object = cursor
                .as_object()
                .ok_or_else(|| EvmError::explorer("Invalid pagination cursor"))?;
            let params: Vec<_> = object
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        value
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| value.to_string()),
                    )
                })
                .collect();
            request = request.query(&params);
        }
        let response = request
            .send()
            .await
            .map_err(|e| EvmError::explorer(e.without_url().to_string()))?
            .error_for_status()
            .map_err(|e| EvmError::explorer(e.without_url().to_string()))?;
        let page: Page = response
            .json()
            .await
            .map_err(|e| EvmError::explorer(format!("Invalid Blockscout page: {e}")))?;
        all.items.extend(page.items);
        all.next_page_params = page
            .next_page_params
            .filter(|v| !v.is_null() && v.as_object().is_none_or(|o| !o.is_empty()));
        if all.next_page_params.is_none() || page_index == max_pages {
            return Ok((all, page_index));
        }
        if !seen.insert(all.next_page_params.as_ref().unwrap().to_string()) {
            return Err(EvmError::explorer(
                "Explorer returned a repeating pagination cursor",
            ));
        }
    }
    unreachable!()
}
