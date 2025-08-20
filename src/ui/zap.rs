use anyhow::{anyhow, Result};
use nostr::{
    nips::{
        nip04,
        nip47::{NostrWalletConnectURI, PayInvoiceRequest, Request, RequestParams},
    },
    EventBuilder, Keys, Kind, PublicKey, RelayUrl, Tag,
};
use nostr_sdk::Client;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use ureq;

#[derive(Debug, Serialize, Deserialize)]
struct LnurlPayResponse {
    callback: String,
    #[serde(rename = "maxSendable")]
    max_sendable: u64,
    #[serde(rename = "minSendable")]
    min_sendable: u64,
    metadata: String,
    tag: String,
    #[serde(default)]
    #[serde(rename = "allowsNostr")]
    allows_nostr: bool,
    #[serde(default)]
    #[serde(rename = "nostrPubkey")]
    nostr_pubkey: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct LnurlInvoiceResponse {
    pr: String,
}

fn lud16_to_lnurl(lud16: &str) -> Result<String> {
    let parts: Vec<&str> = lud16.split('@').collect();
    if parts.len() != 2 {
        return Err(anyhow!("Invalid lud16 format"));
    }
    let domain = parts[1];
    let name = parts[0];
    Ok(format!("https://{}/.well-known/lnurlp/{}", domain, name))
}

pub async fn send_zap_request(
    nwc: &NostrWalletConnectURI,
    nwc_client: &Client,
    from_keys: &Keys,
    to_pubkey: PublicKey,
    lud16: &str,
    amount_sats: u64,
    note_id: Option<nostr::EventId>,
    note_kind: Option<Kind>,
) -> Result<()> {
    let amount_msats = amount_sats * 1000;
    let lnurl = lud16_to_lnurl(lud16)?;

    // 1. Fetch LNURL pay parameters
    let lnurl_clone = lnurl.clone();
    let pay_params: LnurlPayResponse = tokio::task::spawn_blocking(move || -> anyhow::Result<LnurlPayResponse> {
        let agent = ureq::agent();
        let res = agent.get(&lnurl_clone).call().map_err(|e| anyhow!(e))?;
        let text = res.into_string().map_err(|e| anyhow!(e))?;
        serde_json::from_str(&text).map_err(|e| anyhow!(e))
    })
    .await??;

    if amount_msats < pay_params.min_sendable || amount_msats > pay_params.max_sendable {
        return Err(anyhow!(
            "Amount must be between {} and {} sats",
            pay_params.min_sendable / 1000,
            pay_params.max_sendable / 1000
        ));
    }

    // 2. Create ZAP request event
    let relays = vec![RelayUrl::from_str("wss://relay.damus.io")?];
    let amount_str = amount_msats.to_string();
    let mut tags = vec![
        Tag::public_key(to_pubkey),
        Tag::parse(["amount", &amount_str])?,
        Tag::relays(relays),
        Tag::parse(["lnurl", &lnurl])?,
    ];
    if let Some(event_id) = note_id {
        tags.push(Tag::event(event_id));
    }
    if let Some(kind) = note_kind {
        tags.push(Tag::parse(["k", &kind.as_u16().to_string()])?);
    }
    let zap_request = EventBuilder::new(Kind::ZapRequest, "")
        .tags(tags)
        .sign(from_keys)
        .await?;
    let zap_request_str = serde_json::to_string(&zap_request)?;

    // 3. Fetch Bolt11 invoice from LNURL callback
    let callback_url = format!(
        "{}?amount={}&nostr={}",
        pay_params.callback,
        amount_msats,
        urlencoding::encode(&zap_request_str)
    );

    let invoice_response: LnurlInvoiceResponse = tokio::task::spawn_blocking(move || -> anyhow::Result<LnurlInvoiceResponse> {
        let agent = ureq::agent();
        let res = agent.get(&callback_url).call().map_err(|e| anyhow!(e))?;
        let text = res.into_string().map_err(|e| anyhow!(e))?;
        serde_json::from_str(&text).map_err(|e| anyhow!(e))
    })
    .await??;
    let invoice = invoice_response.pr;

    // 4. Send pay_invoice request to NWC
    let req = Request {
        method: nostr::nips::nip47::Method::PayInvoice,
        params: RequestParams::PayInvoice(PayInvoiceRequest {
            invoice,
            amount: None,
            id: None,
        }),
    };

    let json_req = serde_json::to_string(&req)?;
    let encrypted_req = nip04::encrypt(&nwc.secret, &nwc.public_key, &json_req)?;

    let event = EventBuilder::new(Kind::WalletConnectRequest, encrypted_req)
        .tags([Tag::public_key(nwc.public_key)])
        .sign(&Keys::new(nwc.secret.clone()))
        .await?;

    nwc_client.send_event(&event).await?;

    println!(
        "ZAP request sent for {} sats to {}. Waiting for confirmation.",
        amount_sats, lud16
    );

    Ok(())
}
