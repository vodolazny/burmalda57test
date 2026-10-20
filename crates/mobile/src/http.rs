// Единственный HTTP-клиент и tokio-рантайм на всё мобильное приложение.
// Core описывает запросы, здесь они превращаются в реальные вызовы reqwest.

use burmalda57_core::{ApiRequest, HttpResult, HttpResponse, TransportError};

pub(crate) struct Http {
    client: reqwest::Client,
    runtime: tokio::runtime::Runtime,
}

impl Http {
    pub(crate) fn new() -> Self {
        let client = reqwest::Client::builder()
            .gzip(true)
            .build()
            .expect("reqwest client");
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("tokio runtime");
        Self { client, runtime }
    }

    pub(crate) fn spawn<F>(&self, future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.runtime.spawn(future);
    }

    pub(crate) fn block_on<F: std::future::Future>(&self, future: F) -> F::Output {
        self.runtime.block_on(future)
    }

    pub(crate) async fn execute(&self, req: &ApiRequest) -> HttpResult {
        let mut builder = if req.method == "POST" {
            self.client.post(&req.url)
        } else {
            self.client.get(&req.url)
        };
        for (name, value) in &req.headers {
            builder = builder.header(name, value);
        }
        builder = builder.timeout(std::time::Duration::from_secs(req.timeout_secs.max(1)));
        if let Some(body) = &req.body {
            builder = builder.body(body.clone());
        }

        let resp = builder.send().await.map_err(|e| {
            if e.is_timeout() {
                TransportError::Timeout
            } else {
                TransportError::Offline
            }
        })?;
        let status = resp.status().as_u16();
        let body = resp
            .bytes()
            .await
            .map_err(|_| TransportError::Offline)?
            .to_vec();
        Ok(HttpResponse { status, body })
    }
}
