#[cfg(test)]
mod tests;
pub mod types;

use std::collections::BTreeMap;

use bytes::Bytes;
use derive_more::{Display, From};

use binding_macro::{cycles, genesis, service, write};
use protocol::traits::{ExecutorParams, ServiceSDK, StoreMap};
use protocol::types::{Address, Hash, ServiceContext};
use protocol::{ProtocolError, ProtocolErrorKind, ProtocolResult};

use crate::types::{
    ApproveEvent, ApprovePayload, Asset, AssetBalance, BurnTokenPayload, CreateAssetPayload,
    GetAllowancePayload, GetAllowanceResponse, GetAssetPayload, GetBalancePayload,
    GetBalanceResponse, InitGenesisPayload, MintTokenPayload, TransferEvent, TransferFromEvent,
    TransferFromPayload, TransferPayload,
};

const NATIVE_ASSET_KEY: &str = "native_asset";

pub struct AssetService<SDK> {
    sdk:    SDK,
    assets: Box<dyn StoreMap<Hash, Asset>>,
}

#[service]
impl<SDK: ServiceSDK> AssetService<SDK> {
    pub fn new(mut sdk: SDK) -> ProtocolResult<Self> {
        let assets: Box<dyn StoreMap<Hash, Asset>> = sdk.alloc_or_recover_map("assets")?;

        Ok(Self { sdk, assets })
    }

    #[genesis]
    fn init_genesis(&mut self, payload: InitGenesisPayload) -> ProtocolResult<()> {
        let asset = Asset {
            id:     payload.id.clone(),
            name:   payload.name,
            supply: payload.supply,
            issuer: payload.issuer.clone(),
        };

        self.assets.insert(asset.id.clone(), asset.clone())?;
        self.sdk
            .set_value(NATIVE_ASSET_KEY.to_owned(), payload.id.clone())?;

        let asset_balance = AssetBalance {
            value:     payload.supply,
            allowance: BTreeMap::new(),
        };

        self.sdk
            .set_account_value(&asset.issuer, asset.id, asset_balance)
    }

    #[cycles(100_00)]
    #[read]
    fn get_native_asset(&self, ctx: ServiceContext) -> ProtocolResult<Asset> {
        let asset_id: Hash = self
            .sdk
            .get_value(&NATIVE_ASSET_KEY.to_owned())?
            .expect("native asset id should not be empty");

        self.assets.get(&asset_id)
    }

    #[cycles(100_00)]
    #[read]
    fn get_asset(&self, ctx: ServiceContext, payload: GetAssetPayload) -> ProtocolResult<Asset> {
        let asset = self.assets.get(&payload.id)?;
        Ok(asset)
    }

    #[cycles(100_00)]
    #[read]
    fn get_balance(
        &self,
        ctx: ServiceContext,
        payload: GetBalancePayload,
    ) -> ProtocolResult<GetBalanceResponse> {
        if !self.assets.contains(&payload.asset_id)? {
            return Err(ServiceError::NotFoundAsset {
                id: payload.asset_id,
            }
            .into());
        }

        let asset_balance = self
            .sdk
            .get_account_value(&payload.user, &payload.asset_id)?
            .unwrap_or(AssetBalance {
                value:     0,
                allowance: BTreeMap::new(),
            });

        Ok(GetBalanceResponse {
            asset_id: payload.asset_id,
            user:     payload.user,
            balance:  asset_balance.value,
        })
    }

    #[cycles(100_00)]
    #[read]
    fn get_allowance(
        &self,
        ctx: ServiceContext,
        payload: GetAllowancePayload,
    ) -> ProtocolResult<GetAllowanceResponse> {
        if !self.assets.contains(&payload.asset_id)? {
            return Err(ServiceError::NotFoundAsset {
                id: payload.asset_id,
            }
            .into());
        }

        let opt_asset_balance: Option<AssetBalance> = self
            .sdk
            .get_account_value(&payload.grantor, &payload.asset_id)?;

        if let Some(v) = opt_asset_balance {
            let allowance = v.allowance.get(&payload.grantee).unwrap_or(&0);

            Ok(GetAllowanceResponse {
                asset_id: payload.asset_id,
                grantor:  payload.grantor,
                grantee:  payload.grantee,
                value:    *allowance,
            })
        } else {
            Ok(GetAllowanceResponse {
                asset_id: payload.asset_id,
                grantor:  payload.grantor,
                grantee:  payload.grantee,
                value:    0,
            })
        }
    }

    #[write]
    fn mint_token(&mut self, ctx: ServiceContext, payload: MintTokenPayload) -> ProtocolResult<()> {
        if ctx.get_extra().is_none() {
            return Err(ServiceError::NoPermission.into());
        }

        let token_id = payload.token_id;

        if !self.assets.contains(&token_id)? {
            let asset = Asset {
                id:     token_id.clone(),
                name:   "ckb-image_token".to_owned() + &token_id.as_hex().as_str()[2..7],
                supply: 0,
                issuer: Address::from_hex("0xc4b0000000000000000000000000000000000000")?,
            };
            self.assets.insert(token_id.clone(), asset.clone())?;
            let asset_balance = AssetBalance {
                value:     payload.amount,
                allowance: BTreeMap::new(),
            };
            self.sdk
                .set_account_value(&payload.receiver, asset.id.clone(), asset_balance)?;
        } else {
            let mut receiver_balance: AssetBalance = self
                .sdk
                .get_account_value(&payload.receiver, &token_id)?
                .unwrap_or(AssetBalance {
                    value:     0,
                    allowance: BTreeMap::new(),
                });

            let (v, overflow) = receiver_balance.value.overflowing_add(payload.amount);
            if overflow {
                return Err(ServiceError::U128Overflow.into());
            }
            receiver_balance.value = v;

            self.sdk
                .set_account_value(&payload.receiver, token_id.clone(), receiver_balance)?;
        }

        Ok(())
    }

    #[write]
    fn burn_token(&mut self, ctx: ServiceContext, payload: BurnTokenPayload) -> ProtocolResult<()> {
        if ctx.get_extra().is_none() {
            return Err(ServiceError::NoPermission.into());
        }
        if !self.assets.contains(&payload.token_id)? {
            return Err(ServiceError::NotFoundAsset {
                id: payload.token_id,
            }
            .into());
        }

        let mut user_asset_balance: AssetBalance = self
            .sdk
            .get_account_value(&payload.user, &payload.token_id)?
            .unwrap_or(AssetBalance {
                value:     0,
                allowance: BTreeMap::new(),
            });
        let user_balance = user_asset_balance.value;

        if user_balance < payload.amount {
            return Err(ServiceError::LackOfBalance {
                expect: payload.amount,
                real:   user_balance,
            }
            .into());
        }

        user_asset_balance.value = user_balance - payload.amount;
        self.sdk
            .set_account_value(&payload.user, payload.token_id.clone(), user_asset_balance)
    }

    #[cycles(210_00)]
    #[write]
    fn create_asset(
        &mut self,
        ctx: ServiceContext,
        payload: CreateAssetPayload,
    ) -> ProtocolResult<Asset> {
        let caller = ctx.get_caller();
        let payload_str = serde_json::to_string(&payload).map_err(ServiceError::JsonParse)?;

        let id = Hash::digest(Bytes::from(payload_str + &caller.as_hex()));

        if self.assets.contains(&id)? {
            return Err(ServiceError::Exists { id }.into());
        }
        let asset = Asset {
            id:     id.clone(),
            name:   payload.name,
            supply: payload.supply,
            issuer: caller,
        };
        self.assets.insert(id, asset.clone())?;

        let asset_balance = AssetBalance {
            value:     payload.supply,
            allowance: BTreeMap::new(),
        };

        self.sdk
            .set_account_value(&asset.issuer, asset.id.clone(), asset_balance)?;

        let event_str = serde_json::to_string(&asset).map_err(ServiceError::JsonParse)?;
        ctx.emit_event(event_str)?;

        Ok(asset)
    }

    #[cycles(210_00)]
    #[write]
    fn transfer(&mut self, ctx: ServiceContext, payload: TransferPayload) -> ProtocolResult<()> {
        let sender = if let Some(addr_hex) = ctx.get_extra() {
            Address::from_hex(&String::from_utf8(addr_hex.to_vec()).expect("extra should be hex"))?
        } else {
            ctx.get_caller()
        };

        let asset_id = payload.asset_id.clone();
        let value = payload.value;
        let to = payload.to;

        if !self.assets.contains(&asset_id)? {
            return Err(ServiceError::NotFoundAsset { id: asset_id }.into());
        }

        self._transfer(sender.clone(), to.clone(), asset_id.clone(), value)?;

        let event = TransferEvent {
            asset_id,
            from: sender,
            to,
            value,
        };
        let event_str = serde_json::to_string(&event).map_err(ServiceError::JsonParse)?;
        ctx.emit_event(event_str)
    }

    #[cycles(210_00)]
    #[write]
    fn approve(&mut self, ctx: ServiceContext, payload: ApprovePayload) -> ProtocolResult<()> {
        let caller = ctx.get_caller();
        let asset_id = payload.asset_id.clone();
        let value = payload.value;
        let to = payload.to;

        if caller == to {
            return Err(ServiceError::ApproveToYourself.into());
        }

        if !self.assets.contains(&asset_id)? {
            return Err(ServiceError::NotFoundAsset { id: asset_id }.into());
        }

        let mut caller_asset_balance: AssetBalance = self
            .sdk
            .get_account_value(&caller, &asset_id)?
            .unwrap_or(AssetBalance {
                value:     0,
                allowance: BTreeMap::new(),
            });
        caller_asset_balance
            .allowance
            .entry(to.clone())
            .and_modify(|e| *e = value)
            .or_insert(value);

        self.sdk
            .set_account_value(&caller, asset_id.clone(), caller_asset_balance)?;

        let event = ApproveEvent {
            asset_id,
            grantor: caller,
            grantee: to,
            value,
        };
        let event_str = serde_json::to_string(&event).map_err(ServiceError::JsonParse)?;
        ctx.emit_event(event_str)
    }

    #[cycles(210_00)]
    #[write]
    fn transfer_from(
        &mut self,
        ctx: ServiceContext,
        payload: TransferFromPayload,
    ) -> ProtocolResult<()> {
        let caller = if let Some(addr_hex) = ctx.get_extra() {
            Address::from_hex(&String::from_utf8(addr_hex.to_vec()).expect("extra should be hex"))?
        } else {
            ctx.get_caller()
        };
        let sender = payload.sender;
        let recipient = payload.recipient;
        let asset_id = payload.asset_id;
        let value = payload.value;

        if !self.assets.contains(&asset_id)? {
            return Err(ServiceError::NotFoundAsset { id: asset_id }.into());
        }

        let mut sender_asset_balance: AssetBalance = self
            .sdk
            .get_account_value(&sender, &asset_id)?
            .unwrap_or(AssetBalance {
                value:     0,
                allowance: BTreeMap::new(),
            });
        let sender_allowance = sender_asset_balance
            .allowance
            .entry(caller.clone())
            .or_insert(0);
        if *sender_allowance < value {
            return Err(ServiceError::LackOfBalance {
                expect: value,
                real:   *sender_allowance,
            }
            .into());
        }
        let after_sender_allowance = *sender_allowance - value;
        sender_asset_balance
            .allowance
            .entry(caller.clone())
            .and_modify(|e| *e = after_sender_allowance)
            .or_insert(after_sender_allowance);
        self.sdk
            .set_account_value(&sender, asset_id.clone(), sender_asset_balance)?;

        self._transfer(sender.clone(), recipient.clone(), asset_id.clone(), value)?;

        let event = TransferFromEvent {
            asset_id,
            caller,
            sender,
            recipient,
            value,
        };
        let event_str = serde_json::to_string(&event).map_err(ServiceError::JsonParse)?;
        ctx.emit_event(event_str)
    }

    fn _transfer(
        &mut self,
        sender: Address,
        recipient: Address,
        asset_id: Hash,
        value: u128,
    ) -> ProtocolResult<()> {
        if sender == recipient {
            return Err(ServiceError::RecipientIsSender.into());
        }

        let mut sender_asset_balance: AssetBalance = self
            .sdk
            .get_account_value(&sender, &asset_id)?
            .unwrap_or(AssetBalance {
                value:     0,
                allowance: BTreeMap::new(),
            });
        let sender_balance = sender_asset_balance.value;

        if sender_balance < value {
            return Err(ServiceError::LackOfBalance {
                expect: value,
                real:   sender_balance,
            }
            .into());
        }

        let mut to_asset_balance: AssetBalance = self
            .sdk
            .get_account_value(&recipient, &asset_id)?
            .unwrap_or(AssetBalance {
                value:     0,
                allowance: BTreeMap::new(),
            });

        let (v, overflow) = to_asset_balance.value.overflowing_add(value);
        if overflow {
            return Err(ServiceError::U128Overflow.into());
        }
        to_asset_balance.value = v;

        self.sdk
            .set_account_value(&recipient, asset_id.clone(), to_asset_balance)?;

        let (v, overflow) = sender_balance.overflowing_sub(value);
        if overflow {
            return Err(ServiceError::U128Overflow.into());
        }
        sender_asset_balance.value = v;
        self.sdk
            .set_account_value(&sender, asset_id, sender_asset_balance)?;

        Ok(())
    }
}

#[derive(Debug, Display, From)]
pub enum ServiceError {
    #[display(fmt = "Parsing payload to json failed {:?}", _0)]
    JsonParse(serde_json::Error),

    #[display(fmt = "Asset {:?} already exists", id)]
    Exists {
        id: Hash,
    },

    #[display(fmt = "Not found asset, id {:?}", id)]
    NotFoundAsset {
        id: Hash,
    },

    #[display(fmt = "Not found asset, expect {:?} real {:?}", expect, real)]
    LackOfBalance {
        expect: u128,
        real:   u128,
    },

    FeeNotEnough,

    U128Overflow,

    RecipientIsSender,

    ApproveToYourself,

    NoPermission,
}

impl std::error::Error for ServiceError {}

impl From<ServiceError> for ProtocolError {
    fn from(err: ServiceError) -> ProtocolError {
        ProtocolError::new(ProtocolErrorKind::Service, Box::new(err))
    }
}
