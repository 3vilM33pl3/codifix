// SPDX-License-Identifier: AGPL-3.0-or-later
use super::{
    CompanyOrigin,
    credentials::{Enrollment, sha256},
};
use anyhow::{Result, bail};
use mem_record::{
    OnboardingDevicePollRequest, OnboardingDevicePollResponse, OnboardingDeviceReviewProject,
    OnboardingDeviceStartRequest, OnboardingDeviceStartResponse,
};
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::time::Duration;

pub(super) struct CompanyClient {
    client: Client,
    origin: CompanyOrigin,
}
#[derive(Serialize, Deserialize)]
pub(super) struct AvailableProjects {
    pub(super) schema_version: u32,
    pub(super) projects: Vec<OnboardingDeviceReviewProject>,
}

impl CompanyClient {
    #[cfg(feature = "project-sync")]
    pub(super) async fn subscription(
        &self,
        enrollment: &Enrollment,
        project: uuid::Uuid,
        area: &str,
        owner: uuid::Uuid,
    ) -> Result<mem_federation_runtime::DeviceProjectSubscription> {
        if project.is_nil() || owner.is_nil() || !matches!(area, "workshop" | "homelab" | "general")
        {
            bail!("exact project identity required");
        }
        let response = self
            .client
            .get(self.origin.endpoint(&format!(
                "/v1/onboarding/areas/{area}/owners/{owner}/projects/{project}/subscription"
            )))
            .bearer_auth(&enrollment.credential)
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("authorized subscription bootstrap unavailable"))?;
        read_metadata(response).await
    }
    pub(super) fn new(origin: CompanyOrigin) -> Result<Self> {
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|_| anyhow::anyhow!("could not build isolated company HTTP client"))?;
        Ok(Self { client, origin })
    }
    pub(super) async fn start(
        &self,
        enrollment: &Enrollment,
    ) -> Result<OnboardingDeviceStartResponse> {
        let request = OnboardingDeviceStartRequest {
            request_id: enrollment.request_id,
            device_id: enrollment.device_id,
            name: enrollment.name.clone(),
            platform: enrollment.platform.clone(),
            credential_sha256: sha256(&enrollment.credential),
            poll_sha256: sha256(&enrollment.poll_secret),
        };
        let response = self.client.post(self.origin.endpoint("/v1/onboarding/device/start")).json(&request).send().await
            .map_err(|_| anyhow::anyhow!("enrollment start unconfirmed; repeat the same login to resume the saved request"))?;
        read_metadata(response).await
    }
    pub(super) async fn poll(
        &self,
        enrollment: &Enrollment,
    ) -> Result<OnboardingDevicePollResponse> {
        let response = self.client.post(self.origin.endpoint("/v1/onboarding/device/poll"))
            .json(&OnboardingDevicePollRequest { request_id: enrollment.request_id, poll_secret: enrollment.poll_secret.clone() }).send().await
            .map_err(|_| anyhow::anyhow!("enrollment poll unconfirmed; repeat the same login to resume without generating a new credential"))?;
        read_metadata(response).await
    }
    pub(super) async fn projects(&self, enrollment: &Enrollment) -> Result<AvailableProjects> {
        let response = self
            .client
            .get(self.origin.endpoint("/v1/onboarding/projects"))
            .bearer_auth(&enrollment.credential)
            .send()
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "company project inventory unavailable; saved device credential retained"
                )
            })?;
        let projects: AvailableProjects = read_metadata(response).await?;
        if projects.schema_version != 1 {
            bail!("unsupported company project catalog version");
        }
        for project in &projects.projects {
            if project.allowed_roles.iter().any(|role| {
                !matches!(
                    role,
                    mem_record::AuthRole::Reader | mem_record::AuthRole::Writer
                )
            }) || (project.manifest.entry.area == mem_record::OnboardingArea::Trust
                && (project.offline_available || project.manifest.entry.offline_allowed))
            {
                bail!(
                    "company project catalog contains an unsupported role or Trust offline policy"
                );
            }
        }
        Ok(projects)
    }
    pub(super) async fn logout(&self, enrollment: &Enrollment) -> Result<()> {
        let response = self
            .client
            .post(self.origin.endpoint("/v1/onboarding/device/logout"))
            .bearer_auth(&enrollment.credential)
            .json(&serde_json::json!({}))
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("server device revocation unconfirmed"))?;
        #[derive(Deserialize)]
        struct Receipt {
            logged_out: bool,
            device_id: uuid::Uuid,
        }
        let receipt = read_metadata::<Receipt>(response).await?;
        if !receipt.logged_out || receipt.device_id != enrollment.device_id {
            bail!("server device revocation identity unconfirmed");
        }
        Ok(())
    }
    /// A conflict means the credential may have been issued: only the device
    /// credential's logout proof can acknowledge that revocation.
    pub(super) async fn cancel(&self, enrollment: &Enrollment) -> Result<bool> {
        let response = self
            .client
            .post(self.origin.endpoint("/v1/onboarding/device/cancel"))
            .json(&OnboardingDevicePollRequest {
                request_id: enrollment.request_id,
                poll_secret: enrollment.poll_secret.clone(),
            })
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("server enrollment cancellation unconfirmed"))?;
        if response.status() == reqwest::StatusCode::CONFLICT {
            return Ok(false);
        }
        #[derive(Deserialize)]
        struct Receipt {
            status: String,
            request_id: uuid::Uuid,
            device_id: uuid::Uuid,
        }
        let receipt = read_metadata::<Receipt>(response).await?;
        if receipt.status != "denied"
            || receipt.request_id != enrollment.request_id
            || receipt.device_id != enrollment.device_id
        {
            bail!("server enrollment cancellation identity unconfirmed");
        }
        Ok(true)
    }
}

async fn read_metadata<T: DeserializeOwned>(mut response: Response) -> Result<T> {
    let status = response.status();
    if !status.is_success() {
        bail!(
            "company request failed with HTTP {}; response body suppressed, saved credentials retained",
            status.as_u16()
        );
    }
    const MAX_BYTES: usize = 256 * 1024;
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| anyhow::anyhow!("company response interrupted; saved credentials retained"))?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_BYTES {
            bail!("company metadata response exceeds the size limit; saved credentials retained");
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("company metadata response unreadable; response body suppressed, saved credentials retained"))
}

pub(super) fn approval_url(origin: &CompanyOrigin, code: &str) -> Result<String> {
    if code.len() != 9
        || code.as_bytes()[4] != b'-'
        || !code.bytes().enumerate().all(|(index, value)| {
            index == 4 || value.is_ascii_uppercase() || value.is_ascii_digit()
        })
    {
        bail!("company returned an invalid public user code; saved request retained");
    }
    Ok(origin.endpoint(&format!("/?device_code={code}")))
}

pub(super) fn validated_approval_url(
    origin: &CompanyOrigin,
    code: &str,
    received: &str,
) -> Result<String> {
    let local = approval_url(origin, code)?;
    if received == local {
        return Ok(local);
    }
    let company = format!("{local}&company_device=1");
    if received == company {
        return Ok(company);
    }
    bail!("enrollment approval URL mismatch; the saved request is retained")
}
pub(super) fn open_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("open").arg(url).status()
            .map_err(|_| anyhow::anyhow!("could not open browser; visit the printed public approval URL or resume login with --no-browser"))?;
        if !status.success() {
            bail!("browser could not open; use the printed public approval URL");
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        let open: Vec<u16> = "open".encode_utf16().chain([0]).collect();
        let url: Vec<u16> = url.encode_utf16().chain([0]).collect();
        // SAFETY: validated company approval URL; no shell interpolation,
        // credentials, executable selection or inherited application handles.
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                open.as_ptr(),
                url.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
            )
        };
        if result as isize <= 32 {
            bail!("browser could not open; use the printed public approval URL");
        }
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = url;
        bail!("automatic browser launch is supported only on macOS; use --no-browser")
    }
}
