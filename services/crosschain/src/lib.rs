pub mod types;

use std::collections::BTreeMap;

use byteorder::{ByteOrder, LittleEndian};
use bytes::Bytes;
use derive_more::{Display, From};

use binding_macro::{cycles, genesis, service, write};
use protocol::traits::{ExecutorParams, ServiceSDK, StoreMap, StoreUint64};
use protocol::types::{Address, Hash, ServiceContext};
use protocol::{ProtocolError, ProtocolErrorKind, ProtocolResult};

use crate::types::{
    BurnCallAssetPayload, BurnPayload, BurnTokenEvent, CkbHeader, CkbHeaderInner, CkbTx,
    MessagePayload, MintTokenEvent, MintTokenPayload, UpdateHeadersPayload,
};

static ADMISSION_TOKEN: Bytes = Bytes::from_static(b"crosschain");
static SUDT_CODE_HASH: &str = "0x57dd0067814dab356e05c6def0d094bb79776711e68ffdfad2df6a7f877f7db6";

pub struct CrosschainService<SDK> {
    sdk:             SDK,
    headers:         Box<dyn StoreMap<u64, CkbHeaderInner>>,
    effected_proofs: Box<dyn StoreMap<Hash, bool>>,
    nonce:           Box<dyn StoreUint64>,
}

#[service]
impl<SDK: ServiceSDK> CrosschainService<SDK> {
    pub fn new(mut sdk: SDK) -> ProtocolResult<Self> {
        let headers: Box<dyn StoreMap<u64, CkbHeaderInner>> =
            sdk.alloc_or_recover_map("headers")?;
        let effected_proofs: Box<dyn StoreMap<Hash, bool>> =
            sdk.alloc_or_recover_map("effected_proofs")?;
        let nonce: Box<dyn StoreUint64> = sdk.alloc_or_recover_uint64("nonce")?;

        Ok(Self {
            sdk,
            headers,
            effected_proofs,
            nonce,
        })
    }

    #[genesis]
    fn init_genesis(&mut self) -> ProtocolResult<()> {
        self.nonce.set(0)
    }

    #[write]
    fn update_headers(
        &mut self,
        _ctx: ServiceContext,
        payload: UpdateHeadersPayload,
    ) -> ProtocolResult<()> {
        for h in payload.headers.into_iter() {
            let inner_header =
                CkbHeaderInner::from(h).map_err(|_| ServiceError::InvalidCrossHeader)?;
            let height = inner_header.number;
            self.headers.insert(height, inner_header)?;
        }

        Ok(())
    }

    #[write]
    fn submit_messages(
        &mut self,
        ctx: ServiceContext,
        payload: MessagePayload,
    ) -> ProtocolResult<()> {
        for m in payload.messages.into_iter() {
            let tx = m.tx;
            self.check_tx(&tx)?;
            let token_id = Hash::from_hex(
                &tx.outputs[0]
                    .clone()
                    .type_
                    .unwrap()
                    .args
                    .as_string()
                    .as_str(),
            )?;
            let amount_bytes = hex::decode(tx.outputs_data[0].as_string_trim0x().as_str())
                .map_err(|_| ServiceError::InvalidCrossTx)?;
            let mut amount: u128 = LittleEndian::read_u128(amount_bytes.as_slice());
            let receiver: Address =
                Address::from_hex(&tx.witnesses.last().unwrap().clone().as_string().as_str())?;

            let amount_relay = amount / 100;
            amount -= amount_relay;
            let mint_payload = MintTokenPayload {
                token_id: token_id.clone(),
                receiver: receiver.clone(),
                amount,
            };
            let payload_string =
                serde_json::to_string(&mint_payload).map_err(ServiceError::JsonParse)?;
            self.sdk.write(
                &ctx,
                Some(ADMISSION_TOKEN.clone()),
                "asset",
                "mint_token",
                &payload_string,
            )?;

            let event = MintTokenEvent {
                asset_id: token_id.clone(),
                asset_name: "ckb_image_token".to_owned() + &token_id.as_hex().as_str()[2..7],
                receiver: receiver.clone(),
                amount,
                kind: "cross_to_muta".to_owned(),
                topic: "mint_asset".to_owned(),
            };
            let event_str = serde_json::to_string(&event).map_err(ServiceError::JsonParse)?;
            ctx.emit_event(event_str)?;

            let mint_relay_payload = MintTokenPayload {
                token_id: token_id.clone(),
                receiver: ctx.get_caller(),
                amount:   amount_relay,
            };
            let relay_payload_string =
                serde_json::to_string(&mint_relay_payload).map_err(ServiceError::JsonParse)?;
            self.sdk.write(
                &ctx,
                Some(ADMISSION_TOKEN.clone()),
                "asset",
                "mint_token",
                &relay_payload_string,
            )?;

            let event_relay = MintTokenEvent {
                asset_id:   token_id.clone(),
                asset_name: "ckb_image_token".to_owned() + &token_id.as_hex().as_str()[2..7],
                receiver:   ctx.get_caller(),
                amount:     amount_relay,
                kind:       "cross_to_muta".to_owned(),
                topic:      "mint_asset".to_owned(),
            };
            let event_relay_str =
                serde_json::to_string(&event_relay).map_err(ServiceError::JsonParse)?;
            ctx.emit_event(event_relay_str)?;
        }

        Ok(())
    }

    #[write]
    fn burn_sudt(&mut self, ctx: ServiceContext, payload: BurnPayload) -> ProtocolResult<()> {
        let call_asset_payload = BurnCallAssetPayload {
            token_id: payload.token_id.clone(),
            user:     ctx.get_caller(),
            amount:   payload.amount,
        };
        let payload_string =
            serde_json::to_string(&call_asset_payload).map_err(ServiceError::JsonParse)?;
        self.sdk.write(
            &ctx,
            Some(ADMISSION_TOKEN.clone()),
            "asset",
            "burn_token",
            &payload_string,
        )?;

        self.nonce.add(1)?;

        let event = BurnTokenEvent {
            asset_id:     payload.token_id.clone(),
            muta_sender:  ctx.get_caller(),
            ckb_receiver: payload.receiver.clone(),
            amount:       payload.amount,
            nonce:        self.nonce.get()?,
            kind:         "cross_to_ckb".to_owned(),
            topic:        "burn_asset".to_owned(),
        };
        let event_str = serde_json::to_string(&event).map_err(ServiceError::JsonParse)?;
        ctx.emit_event(event_str)?;
        Ok(())
    }

    fn check_tx(&self, tx: &CkbTx) -> ProtocolResult<()> {
        let output = &tx.outputs[0];
        if output.type_.is_none()
            || (output.type_.clone().unwrap().code_hash != Hash::from_hex(SUDT_CODE_HASH)?)
            || tx.witnesses.is_empty()
        {
            return Err(ServiceError::InvalidCrossTx.into());
        }

        Ok(())
    }
}

#[derive(Debug, Display, From)]
pub enum ServiceError {
    #[display(fmt = "Parsing payload to json failed {:?}", _0)]
    JsonParse(serde_json::Error),

    InvalidCrossTx,

    InvalidCrossHeader,
}

impl std::error::Error for ServiceError {}

impl From<ServiceError> for ProtocolError {
    fn from(err: ServiceError) -> ProtocolError {
        ProtocolError::new(ProtocolErrorKind::Service, Box::new(err))
    }
}
