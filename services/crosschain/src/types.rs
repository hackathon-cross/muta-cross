use std::mem;
use std::num::ParseIntError;

use byteorder::{ByteOrder, LittleEndian};
use bytes::{Bytes, BytesMut};
use serde::{Deserialize, Serialize};

use protocol::fixed_codec::{FixedCodec, FixedCodecError};
use protocol::types::{Address, Hash, Hex};
use protocol::ProtocolResult;

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct UpdateHeadersPayload {
    pub headers: Vec<CkbHeader>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct CkbHeader {
    pub compact_target:    Hex,
    pub version:           Hex,
    pub timestamp:         Hex,
    pub number:            Hex,
    pub epoch:             Hex,
    pub parent_hash:       Hash,
    pub transactions_root: Hash,
    pub proposals_hash:    Hash,
    pub uncles_hash:       Hash,
    pub dao:               Hash,
    pub nonce:             Hex,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct CkbHeaderInner {
    pub compact_target:    u32,
    pub version:           u32,
    pub timestamp:         u64,
    pub number:            u64,
    pub epoch:             u64,
    pub parent_hash:       Hash,
    pub transactions_root: Hash,
    pub proposals_hash:    Hash,
    pub uncles_hash:       Hash,
    pub dao:               Hash,
    pub nonce:             u128,
}

impl CkbHeaderInner {
    pub fn from(h: CkbHeader) -> Result<Self, ParseIntError> {
        Ok(CkbHeaderInner {
            compact_target:    u32::from_str_radix(
                h.compact_target.as_string_trim0x().as_str(),
                16,
            )?,
            version:           u32::from_str_radix(h.version.as_string_trim0x().as_str(), 16)?,
            timestamp:         u64::from_str_radix(h.timestamp.as_string_trim0x().as_str(), 16)?,
            number:            u64::from_str_radix(h.number.as_string_trim0x().as_str(), 16)?,
            epoch:             u64::from_str_radix(h.epoch.as_string_trim0x().as_str(), 16)?,
            parent_hash:       h.parent_hash,
            transactions_root: h.transactions_root,
            proposals_hash:    h.proposals_hash,
            uncles_hash:       h.uncles_hash,
            dao:               h.dao,
            nonce:             u128::from_str_radix(h.epoch.as_string_trim0x().as_str(), 16)?,
        })
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Uint128(pub u128);

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct BurnPayload {
    pub token_id: Hash,
    pub receiver: String, // hex of ckb address
    pub amount:   u128,   // amount of asset to cross-back to ckb
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct BurnCallAssetPayload {
    pub token_id: Hash,
    pub user:     Address,
    pub amount:   u128,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct BurnTokenEvent {
    pub asset_id:     Hash,
    pub muta_sender:  Address,
    pub ckb_receiver: String,
    pub amount:       u128,
    pub nonce:        u64,
    pub kind:         String, // "cross_to_ckb"
    pub topic:        String, // "burn_asset"
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct MessagePayload {
    pub height:   u64, // ckb block height
    pub messages: Vec<CkbMessage>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct CkbMessage {
    pub tx:    CkbTx,
    pub proof: Vec<Hash>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct CkbTx {
    pub version:      Hex,
    pub cell_deps:    Vec<CellDep>,
    pub header_deps:  Vec<Hash>,
    pub inputs:       Vec<CellInput>,
    pub outputs:      Vec<CellOutput>,
    pub outputs_data: Vec<Hex>,
    pub witnesses:    Vec<Hex>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct CellDep {
    pub out_point: OutPoint,
    pub dep_type:  DepType,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub enum DepType {
    code,
    depgroup,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct OutPoint {
    pub tx_hash: Hash,
    pub index:   Hex,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct CellInput {
    pub since:           Hex,
    pub previous_output: OutPoint,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct CellOutput {
    pub capacity: Hex,
    pub lock:     Script,
    #[serde(rename = "type")]
    pub type_:    Option<Script>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Script {
    pub code_hash: Hash,
    pub hash_type: ScriptHashType,
    pub args:      Hex,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub enum ScriptHashType {
    data,
    #[serde(rename = "type")]
    Type,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct MintTokenPayload {
    pub token_id: Hash,
    pub receiver: Address,
    pub amount:   u128,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct MintTokenEvent {
    pub asset_id:   Hash,   // ckb sudt type args
    pub asset_name: String, //
    pub receiver:   Address,
    pub amount:     u128,
    pub kind:       String, // "cross_to_muta"
    pub topic:      String, // "mint_asset"
}

impl rlp::Decodable for CkbHeaderInner {
    fn decode(rlp: &rlp::Rlp) -> Result<Self, rlp::DecoderError> {
        let buf = rlp.at(10)?.as_raw();
        Ok(Self {
            compact_target:    rlp.at(0)?.as_val()?,
            version:           rlp.at(1)?.as_val()?,
            timestamp:         rlp.at(2)?.as_val()?,
            number:            rlp.at(3)?.as_val()?,
            epoch:             rlp.at(4)?.as_val()?,
            parent_hash:       rlp.at(5)?.as_val()?,
            transactions_root: rlp.at(6)?.as_val()?,
            proposals_hash:    rlp.at(7)?.as_val()?,
            uncles_hash:       rlp.at(8)?.as_val()?,
            dao:               rlp.at(9)?.as_val()?,
            nonce:             LittleEndian::read_u128(&buf),
        })
    }
}

impl rlp::Encodable for CkbHeaderInner {
    fn rlp_append(&self, s: &mut rlp::RlpStream) {
        s.begin_list(11)
            .append(&self.compact_target)
            .append(&self.version)
            .append(&self.timestamp)
            .append(&self.number)
            .append(&self.epoch)
            .append(&self.parent_hash)
            .append(&self.transactions_root)
            .append(&self.proposals_hash)
            .append(&self.uncles_hash)
            .append(&self.dao);

        let mut buf = [0u8; mem::size_of::<u128>()];
        LittleEndian::write_u128(&mut buf, self.nonce);
        s.append(&buf.to_vec());
    }
}

impl FixedCodec for CkbHeaderInner {
    fn encode_fixed(&self) -> ProtocolResult<Bytes> {
        Ok(Bytes::from(rlp::encode(self)))
    }

    fn decode_fixed(bytes: Bytes) -> ProtocolResult<Self> {
        Ok(rlp::decode(bytes.as_ref()).map_err(FixedCodecError::from)?)
    }
}
