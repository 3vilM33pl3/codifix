// SPDX-License-Identifier: AGPL-3.0-or-later
use super::ApiClient;
use crate::commands::output::service_url;
use anyhow::{Result, ensure};
use mem_record::{LaptopCorrectionReceipt, LaptopCorrectionRequest};
use uuid::Uuid;

impl ApiClient {
    pub(crate) async fn stage_laptop_correction(
        &self,
        subscription: Uuid,
        request: &LaptopCorrectionRequest,
    ) -> Result<LaptopCorrectionReceipt> {
        ensure!(
            self.config
                .service
                .bind_addr
                .parse::<std::net::SocketAddr>()
                .is_ok_and(|a| a.ip().is_loopback()),
            "correction staging requires a loopback laptop service"
        );
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        let headers = self.local_session_headers(&client, true).await?;
        let mut response = client
            .post(service_url(
                &self.config,
                &format!("/v1/onboarding/cache/{subscription}/corrections"),
            ))
            .headers(headers)
            .json(request)
            .send()
            .await
            .map_err(|_| {
                anyhow::anyhow!("local correction outcome unavailable; retry the same operation ID")
            })?;
        ensure!(
            response.status().is_success(),
            "correction refused (HTTP {}); source may have changed or local authority may be unavailable",
            response.status().as_u16()
        );
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            ensure!(
                bytes.len() + chunk.len() <= 16384,
                "correction receipt exceeds bounds"
            );
            bytes.extend_from_slice(&chunk);
        }
        let receipt: LaptopCorrectionReceipt = serde_json::from_slice(&bytes)?;
        ensure!(
            receipt.operation_id == request.operation_id && receipt.subscription_id == subscription,
            "correction receipt does not match operation"
        );
        Ok(receipt)
    }
}
