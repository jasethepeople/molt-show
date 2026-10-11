use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type Amount = u64;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourcePool {
    pub pool_id: String,
    pub resource_type: String,
    pub owner_id: String,
    pub capacity: Amount,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Lease {
    pub lease_id: String,
    pub pool_id: String,
    pub parent_lease_id: Option<String>,
    pub grantor_id: String,
    pub grantee_id: String,
    pub amount: Amount,
    pub consumed: Amount,
    pub outstanding_subleases: Amount,
}

impl Lease {
    pub fn available(&self) -> Amount {
        self.amount
            .saturating_sub(self.consumed)
            .saturating_sub(self.outstanding_subleases)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ResourceError {
    #[error("I12: resource pool {0} does not exist")]
    UnknownPool(String),
    #[error("I12: resource pool {0} cannot be created twice")]
    DuplicatePool(String),
    #[error("I13: lease {0} does not exist")]
    UnknownLease(String),
    #[error("I13: lease {0} cannot be created twice")]
    DuplicateLease(String),
    #[error("I13: parent lease {0} does not exist")]
    UnknownParentLease(String),
    #[error("I13: lease amount {requested} exceeds available authority {available}")]
    LeaseConstraint {
        requested: Amount,
        available: Amount,
    },
    #[error("I12: consumption {requested} exceeds available lease authority {available}")]
    ConsumptionConstraint {
        requested: Amount,
        available: Amount,
    },
    #[error("I12: arithmetic overflow")]
    ArithmeticOverflow,
    #[error("invalid resource payload: {0}")]
    InvalidPayload(String),
}

fn field<'a>(
    payload: &'a serde_json::Value,
    name: &str,
) -> Result<&'a serde_json::Value, ResourceError> {
    payload
        .get(name)
        .ok_or_else(|| ResourceError::InvalidPayload(format!("missing {name}")))
}

fn string_field(payload: &serde_json::Value, name: &str) -> Result<String, ResourceError> {
    field(payload, name)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| ResourceError::InvalidPayload(format!("{name} must be a string")))
}

fn amount_field(payload: &serde_json::Value, name: &str) -> Result<Amount, ResourceError> {
    field(payload, name)?.as_u64().ok_or_else(|| {
        ResourceError::InvalidPayload(format!("{name} must be a non-negative integer"))
    })
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceState {
    pub pools: std::collections::BTreeMap<String, ResourcePool>,
    pub leases: std::collections::BTreeMap<String, Lease>,
}

impl ResourceState {
    pub fn apply_event(
        &mut self,
        event_type: &str,
        payload: &serde_json::Value,
        actor_id: &str,
    ) -> Result<(), ResourceError> {
        match event_type {
            "RESOURCE_POOL_CREATED" => {
                let pool_id = string_field(payload, "pool_id")?;
                if self.pools.contains_key(&pool_id) {
                    return Err(ResourceError::DuplicatePool(pool_id));
                }
                let pool = ResourcePool {
                    pool_id: pool_id.clone(),
                    resource_type: string_field(payload, "resource_type")?,
                    owner_id: string_field(payload, "owner_id")?,
                    capacity: amount_field(payload, "capacity")?,
                };
                if pool.owner_id != actor_id {
                    return Err(ResourceError::InvalidPayload(
                        "owner_id must equal event actor_id".into(),
                    ));
                }
                self.pools.insert(pool_id, pool);
            }
            "RESOURCE_LEASE_ISSUED" => {
                let lease_id = string_field(payload, "lease_id")?;
                if self.leases.contains_key(&lease_id) {
                    return Err(ResourceError::DuplicateLease(lease_id));
                }
                let pool_id = string_field(payload, "pool_id")?;
                let amount = amount_field(payload, "amount")?;
                let parent_lease_id = payload
                    .get("parent_lease_id")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned);
                let (grantor_id, available) = if let Some(parent_id) = &parent_lease_id {
                    let parent = self
                        .leases
                        .get(parent_id)
                        .ok_or_else(|| ResourceError::UnknownParentLease(parent_id.clone()))?;
                    (parent.grantee_id.clone(), parent.available())
                } else {
                    let pool = self
                        .pools
                        .get(&pool_id)
                        .ok_or_else(|| ResourceError::UnknownPool(pool_id.clone()))?;
                    (pool.owner_id.clone(), self.pool_available(&pool_id)?)
                };
                if grantor_id != actor_id {
                    return Err(ResourceError::InvalidPayload(
                        "actor_id is not the grantor".into(),
                    ));
                }
                if amount > available {
                    return Err(ResourceError::LeaseConstraint {
                        requested: amount,
                        available,
                    });
                }
                if let Some(parent_id) = &parent_lease_id {
                    let parent = self.leases.get_mut(parent_id).expect("checked above");
                    parent.outstanding_subleases = parent
                        .outstanding_subleases
                        .checked_add(amount)
                        .ok_or(ResourceError::ArithmeticOverflow)?;
                }
                self.leases.insert(
                    lease_id.clone(),
                    Lease {
                        lease_id,
                        pool_id,
                        parent_lease_id,
                        grantor_id,
                        grantee_id: string_field(payload, "grantee_id")?,
                        amount,
                        consumed: 0,
                        outstanding_subleases: 0,
                    },
                );
            }
            "RESOURCE_CONSUMED" => {
                let lease_id = string_field(payload, "lease_id")?;
                let amount = amount_field(payload, "amount")?;
                let lease = self
                    .leases
                    .get_mut(&lease_id)
                    .ok_or_else(|| ResourceError::UnknownLease(lease_id.clone()))?;
                if lease.grantee_id != actor_id {
                    return Err(ResourceError::InvalidPayload(
                        "actor_id is not the lease grantee".into(),
                    ));
                }
                if amount > lease.available() {
                    return Err(ResourceError::ConsumptionConstraint {
                        requested: amount,
                        available: lease.available(),
                    });
                }
                lease.consumed = lease
                    .consumed
                    .checked_add(amount)
                    .ok_or(ResourceError::ArithmeticOverflow)?;
            }
            _ => {}
        }
        Ok(())
    }

    pub fn pool_available(&self, pool_id: &str) -> Result<Amount, ResourceError> {
        let pool = self
            .pools
            .get(pool_id)
            .ok_or_else(|| ResourceError::UnknownPool(pool_id.to_owned()))?;
        let allocated = self
            .leases
            .values()
            .filter(|lease| lease.pool_id == pool_id && lease.parent_lease_id.is_none())
            .try_fold(0u64, |sum, lease| {
                sum.checked_add(lease.amount)
                    .ok_or(ResourceError::ArithmeticOverflow)
            })?;
        Ok(pool.capacity.saturating_sub(allocated))
    }
}
