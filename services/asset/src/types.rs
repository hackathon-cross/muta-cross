use std::collections::BTreeMap;
use std::mem;

use byteorder::{ByteOrder, LittleEndian};
use serde::{Deserialize, Serialize};

use bytes::Bytes;

use protocol::fixed_codec::{FixedCodec, FixedCodecError};
use protocol::types::{Address, Hash};
use protocol::ProtocolResult;

/// Payload
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct InitGenesisPayload {
    pub id:     Hash,
    pub name:   String,
    pub supply: u128,
    pub issuer: Address,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct MintTokenPayload {
    pub token_id: Hash,
    pub receiver: Address,
    pub amount:   u128,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct BurnTokenPayload {
    pub token_id: Hash,
    pub user:     Address,
    pub amount:   u128,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct CreateAssetPayload {
    pub name:   String,
    pub supply: u128,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct GetAssetPayload {
    pub id: Hash,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct TransferPayload {
    pub asset_id: Hash,
    pub to:       Address,
    pub value:    u128,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct TransferEvent {
    pub asset_id: Hash,
    pub from:     Address,
    pub to:       Address,
    pub value:    u128,
}

pub type ApprovePayload = TransferPayload;

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ApproveEvent {
    pub asset_id: Hash,
    pub grantor:  Address,
    pub grantee:  Address,
    pub value:    u128,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct TransferFromPayload {
    pub asset_id:  Hash,
    pub sender:    Address,
    pub recipient: Address,
    pub value:     u128,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct TransferFromEvent {
    pub asset_id:  Hash,
    pub caller:    Address,
    pub sender:    Address,
    pub recipient: Address,
    pub value:     u128,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct GetBalancePayload {
    pub asset_id: Hash,
    pub user:     Address,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct GetBalanceResponse {
    pub asset_id: Hash,
    pub user:     Address,
    pub balance:  u128,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct GetAllowancePayload {
    pub asset_id: Hash,
    pub grantor:  Address,
    pub grantee:  Address,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct GetAllowanceResponse {
    pub asset_id: Hash,
    pub grantor:  Address,
    pub grantee:  Address,
    pub value:    u128,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct Asset {
    pub id:     Hash,
    pub name:   String,
    pub supply: u128,
    pub issuer: Address,
}

pub struct AssetBalance {
    pub value:     u128,
    pub allowance: BTreeMap<Address, u128>,
}

struct AllowanceCodec {
    pub addr:  Address,
    pub total: u128,
}

impl rlp::Decodable for Asset {
    fn decode(rlp: &rlp::Rlp) -> Result<Self, rlp::DecoderError> {
        let buf = rlp.at(2)?.as_raw();
        Ok(Self {
            id:     rlp.at(0)?.as_val()?,
            name:   rlp.at(1)?.as_val()?,
            supply: LittleEndian::read_u128(&buf),
            issuer: rlp.at(3)?.as_val()?,
        })
    }
}

impl rlp::Encodable for Asset {
    fn rlp_append(&self, s: &mut rlp::RlpStream) {
        s.begin_list(4).append(&self.id).append(&self.name);
        let mut buf = [0u8; mem::size_of::<u128>()];
        LittleEndian::write_u128(&mut buf, self.supply);
        s.append(&buf.to_vec()).append(&self.issuer);
    }
}

impl FixedCodec for Asset {
    fn encode_fixed(&self) -> ProtocolResult<Bytes> {
        Ok(Bytes::from(rlp::encode(self)))
    }

    fn decode_fixed(bytes: Bytes) -> ProtocolResult<Self> {
        Ok(rlp::decode(bytes.as_ref()).map_err(FixedCodecError::from)?)
    }
}

impl rlp::Decodable for AllowanceCodec {
    fn decode(rlp: &rlp::Rlp) -> Result<Self, rlp::DecoderError> {
        let buf = rlp.at(1)?.as_raw();
        Ok(Self {
            addr:  rlp.at(0)?.as_val()?,
            total: LittleEndian::read_u128(&buf),
        })
    }
}

impl rlp::Encodable for AllowanceCodec {
    fn rlp_append(&self, s: &mut rlp::RlpStream) {
        s.begin_list(2).append(&self.addr);
        let mut buf = [0u8; mem::size_of::<u128>()];
        LittleEndian::write_u128(&mut buf, self.total);
        s.append(&buf.to_vec());
    }
}

impl rlp::Decodable for AssetBalance {
    fn decode(rlp: &rlp::Rlp) -> Result<Self, rlp::DecoderError> {
        let buf = rlp.at(0)?.as_raw();
        let value = LittleEndian::read_u128(&buf);
        let codec_list: Vec<AllowanceCodec> = rlp::decode_list(rlp.at(1)?.as_raw());
        let mut allowance = BTreeMap::new();
        for v in codec_list {
            allowance.insert(v.addr, v.total);
        }

        Ok(AssetBalance { value, allowance })
    }
}

impl rlp::Encodable for AssetBalance {
    fn rlp_append(&self, s: &mut rlp::RlpStream) {
        s.begin_list(2);
        let mut buf = [0u8; mem::size_of::<u128>()];
        LittleEndian::write_u128(&mut buf, self.value);
        s.append(&buf.to_vec());

        let mut codec_list = Vec::with_capacity(self.allowance.len());

        for (address, allowance) in self.allowance.iter() {
            let fixed_codec = AllowanceCodec {
                addr:  address.clone(),
                total: *allowance,
            };

            codec_list.push(fixed_codec);
        }

        s.append_list(&codec_list);
    }
}

impl FixedCodec for AssetBalance {
    fn encode_fixed(&self) -> ProtocolResult<Bytes> {
        Ok(Bytes::from(rlp::encode(self)))
    }

    fn decode_fixed(bytes: Bytes) -> ProtocolResult<Self> {
        Ok(rlp::decode(bytes.as_ref()).map_err(FixedCodecError::from)?)
    }
}
