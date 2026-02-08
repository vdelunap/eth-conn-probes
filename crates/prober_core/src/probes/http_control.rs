use crate::model;

pub struct HttpControlProbe {
    pub url: String,
    pub expect_body: Option<String>,
}

#[async_trait::async_trait]
impl super::ProbeFn for HttpControlProbe {
    async fn run(&self, timeout_ms: u64) -> model::AttemptResult {
        let started = model::now_ms();

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(timeout_ms))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                return model::AttemptResult {
                    ok: false,
                    rtt_ms: None,
                    error: Some(format!("client_build_error: {e}")),
                    meta: serde_json::json!({}),
                };
            }
        };

        let resp = client.get(&self.url).send().await;
        match resp {
            Ok(r) => {
                let status = r.status().as_u16();
                let body = r.text().await.unwrap_or_default();
                let ok = status == 200
                    && self
                        .expect_body
                        .as_ref()
                        .map(|x| x == &body)
                        .unwrap_or(true);

                model::AttemptResult {
                    ok,
                    rtt_ms: Some(model::now_ms().saturating_sub(started)),
                    error: if ok {
                        None
                    } else {
                        Some(format!("unexpected_response: status={status}"))
                    },
                    meta: serde_json::json!({ "status": status, "body": body }),
                }
            }
            Err(e) => model::AttemptResult {
                ok: false,
                rtt_ms: Some(model::now_ms().saturating_sub(started)),
                error: Some(format!("http_error: {e}")),
                meta: serde_json::json!({}),
            },
        }
    }
}
