//! Deterministic gameplay-framework contracts.
//!
//! These types own reusable state transitions and validation. Rendering, persistence I/O,
//! animation, audio and editor presentation remain separate consumers of the same truth.

use crate::error::{EngineError, Result};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};

pub const GAMEPLAY_CONTRACT_FORMAT: &str = "bhippi-gameplay-contract@1";
pub const INVENTORY_SNAPSHOT_FORMAT: &str = "bhippi-inventory@1";
pub const BASIS_POINTS_MAX: u32 = 10_000;
pub const MILLIS_PER_SECOND: u64 = 1_000;
pub const ATTACHMENT_MODIFIER_LIMIT_BPS: i32 = 10_000;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct ResourcePool {
    pub current: u32,
    pub maximum: u32,
    pub regeneration_per_second: u32,
}

impl ResourcePool {
    pub fn validate(&self, label: &str) -> Result<()> {
        if self.maximum == 0 || self.current > self.maximum {
            return Err(contract_error(
                format!("{label} resource is outside 0..={}", self.maximum),
                "Use a positive maximum and keep current at or below it.",
            ));
        }
        Ok(())
    }

    pub fn spend(&mut self, amount: u32) -> bool {
        if self.current < amount {
            return false;
        }
        self.current -= amount;
        true
    }

    pub fn restore(&mut self, amount: u32) -> u32 {
        let before = self.current;
        self.current = self.current.saturating_add(amount).min(self.maximum);
        self.current - before
    }

    pub fn regenerate(&mut self, elapsed_millis: u64) -> u32 {
        let units = u64::from(self.regeneration_per_second).saturating_mul(elapsed_millis)
            / MILLIS_PER_SECOND;
        self.restore(u32::try_from(units).unwrap_or(u32::MAX))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DamageKind {
    Physical,
    Fire,
    Ice,
    Electric,
    True,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct DamagePacket {
    pub id: String,
    pub source: String,
    pub target: String,
    pub amount: u32,
    pub kind: DamageKind,
    pub bypass_shield: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct CombatResources {
    pub health: ResourcePool,
    pub shield: ResourcePool,
    pub stamina: ResourcePool,
    pub mana: ResourcePool,
    pub alive: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GameplayEvent {
    ResourceChanged {
        entity: String,
        resource: String,
        before: u32,
        after: u32,
    },
    Damaged {
        packet_id: String,
        absorbed_by_shield: u32,
        health_damage: u32,
    },
    Died {
        entity: String,
        source: String,
    },
    Interacted {
        actor: String,
        target: String,
        event: String,
    },
    ObjectiveChanged {
        objective: String,
        state: ObjectiveState,
    },
    WeaponFired {
        weapon: String,
        sequence: u64,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct DamageOutcome {
    pub absorbed_by_shield: u32,
    pub health_damage: u32,
    pub killed: bool,
    pub events: Vec<GameplayEvent>,
}

impl CombatResources {
    pub fn validate(&self) -> Result<()> {
        self.health.validate("health")?;
        self.shield.validate("shield")?;
        self.stamina.validate("stamina")?;
        self.mana.validate("mana")?;
        if self.alive != (self.health.current > 0) {
            return Err(contract_error(
                "combat alive flag disagrees with health".to_owned(),
                "Set alive to true exactly while health is above zero.",
            ));
        }
        Ok(())
    }

    pub fn apply_damage(&mut self, packet: &DamagePacket) -> Result<DamageOutcome> {
        require_text(&packet.id, "damage packet id")?;
        require_text(&packet.source, "damage source")?;
        require_text(&packet.target, "damage target")?;
        if packet.amount == 0 {
            return Err(contract_error(
                "zero damage packet proves no gameplay transition".to_owned(),
                "Use a positive damage amount or omit the packet.",
            ));
        }
        self.validate()?;
        let shield_before = self.shield.current;
        let health_before = self.health.current;
        let absorbed = if packet.bypass_shield || packet.kind == DamageKind::True {
            0
        } else {
            packet.amount.min(self.shield.current)
        };
        self.shield.current -= absorbed;
        let health_damage = packet
            .amount
            .saturating_sub(absorbed)
            .min(self.health.current);
        self.health.current -= health_damage;
        let killed = self.alive && self.health.current == 0;
        self.alive = self.health.current > 0;
        let mut events = Vec::new();
        if shield_before != self.shield.current {
            events.push(GameplayEvent::ResourceChanged {
                entity: packet.target.clone(),
                resource: "shield".to_owned(),
                before: shield_before,
                after: self.shield.current,
            });
        }
        if health_before != self.health.current {
            events.push(GameplayEvent::ResourceChanged {
                entity: packet.target.clone(),
                resource: "health".to_owned(),
                before: health_before,
                after: self.health.current,
            });
        }
        events.push(GameplayEvent::Damaged {
            packet_id: packet.id.clone(),
            absorbed_by_shield: absorbed,
            health_damage,
        });
        if killed {
            events.push(GameplayEvent::Died {
                entity: packet.target.clone(),
                source: packet.source.clone(),
            });
        }
        Ok(DamageOutcome {
            absorbed_by_shield: absorbed,
            health_damage,
            killed,
            events,
        })
    }

    #[must_use]
    pub fn hud_bindings(&self, entity: &str) -> BTreeMap<String, u32> {
        BTreeMap::from([
            (format!("{entity}.health"), self.health.current),
            (format!("{entity}.health_max"), self.health.maximum),
            (format!("{entity}.mana"), self.mana.current),
            (format!("{entity}.mana_max"), self.mana.maximum),
            (format!("{entity}.shield"), self.shield.current),
            (format!("{entity}.shield_max"), self.shield.maximum),
            (format!("{entity}.stamina"), self.stamina.current),
            (format!("{entity}.stamina_max"), self.stamina.maximum),
        ])
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum EquipmentSlot {
    Primary,
    Secondary,
    Head,
    Body,
    Accessory,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct ItemDefinition {
    pub id: String,
    pub name: String,
    pub max_stack: u32,
    #[serde(default)]
    pub equipment_slot: Option<EquipmentSlot>,
    pub persistent: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct ItemStack {
    pub item: String,
    pub amount: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct InventoryState {
    pub capacity_slots: u32,
    #[serde(default)]
    pub stacks: Vec<ItemStack>,
    #[serde(default)]
    pub equipment: BTreeMap<EquipmentSlot, String>,
}

impl InventoryState {
    pub fn validate(&self, catalog: &[ItemDefinition]) -> Result<()> {
        validate_catalog(catalog)?;
        if self.capacity_slots == 0 || self.stacks.len() > self.capacity_slots as usize {
            return Err(contract_error(
                "inventory slot usage exceeds its positive capacity".to_owned(),
                "Increase capacity or remove stacks before loading the inventory.",
            ));
        }
        let definitions = catalog
            .iter()
            .map(|item| (item.id.as_str(), item))
            .collect::<BTreeMap<_, _>>();
        let mut stack_ids = BTreeSet::new();
        for stack in &self.stacks {
            let definition = definitions.get(stack.item.as_str()).ok_or_else(|| {
                contract_error(
                    format!("inventory references unknown item {:?}", stack.item),
                    "Add the item definition or remove the stale stack.",
                )
            })?;
            if stack.amount == 0 || stack.amount > definition.max_stack {
                return Err(contract_error(
                    format!(
                        "item stack {:?} exceeds 1..={}",
                        stack.item, definition.max_stack
                    ),
                    "Split or reduce the stack to the catalog limit.",
                ));
            }
            if !stack_ids.insert(stack.item.as_str()) {
                return Err(contract_error(
                    format!("inventory has duplicate stack {:?}", stack.item),
                    "Merge each item into one bounded stack in this v1 contract.",
                ));
            }
        }
        for (slot, item_id) in &self.equipment {
            let definition = definitions.get(item_id.as_str()).ok_or_else(|| {
                contract_error(
                    format!("equipment references unknown item {item_id:?}"),
                    "Equip an item from the catalog and inventory.",
                )
            })?;
            if definition.equipment_slot.as_ref() != Some(slot)
                || !stack_ids.contains(item_id.as_str())
            {
                return Err(contract_error(
                    format!("item {item_id:?} cannot occupy {slot:?}"),
                    "Equip an owned item in its declared slot.",
                ));
            }
        }
        Ok(())
    }

    pub fn add(&mut self, item_id: &str, amount: u32, catalog: &[ItemDefinition]) -> Result<u32> {
        self.validate(catalog)?;
        if amount == 0 {
            return Ok(0);
        }
        let definition = catalog
            .iter()
            .find(|item| item.id == item_id)
            .ok_or_else(|| {
                contract_error(
                    format!("pickup references unknown item {item_id:?}"),
                    "Use an item id from the validated catalog.",
                )
            })?;
        let mut remainder = amount;
        if let Some(stack) = self.stacks.iter_mut().find(|stack| stack.item == item_id) {
            let room = definition.max_stack.saturating_sub(stack.amount);
            let added = room.min(remainder);
            stack.amount += added;
            remainder -= added;
        } else if self.stacks.len() < self.capacity_slots as usize {
            let added = definition.max_stack.min(remainder);
            self.stacks.push(ItemStack {
                item: item_id.to_owned(),
                amount: added,
            });
            remainder -= added;
        }
        self.stacks
            .sort_by(|left, right| left.item.cmp(&right.item));
        Ok(remainder)
    }

    pub fn equip(&mut self, item_id: &str, catalog: &[ItemDefinition]) -> Result<()> {
        self.validate(catalog)?;
        if !self.stacks.iter().any(|stack| stack.item == item_id) {
            return Err(contract_error(
                format!("cannot equip unowned item {item_id:?}"),
                "Pick up the item before equipping it.",
            ));
        }
        let definition = catalog
            .iter()
            .find(|item| item.id == item_id)
            .ok_or_else(|| {
                contract_error(
                    format!("cannot equip unknown item {item_id:?}"),
                    "Use an item from the catalog.",
                )
            })?;
        let slot = definition.equipment_slot.ok_or_else(|| {
            contract_error(
                format!("item {item_id:?} is not equippable"),
                "Choose an item with an equipment_slot.",
            )
        })?;
        self.equipment.insert(slot, item_id.to_owned());
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct InventorySnapshot {
    pub format: String,
    pub owner: String,
    pub inventory: InventoryState,
}

impl InventorySnapshot {
    pub fn parse(text: &str, catalog: &[ItemDefinition]) -> Result<Self> {
        let snapshot: Self = serde_json::from_str(text).map_err(|error| {
            contract_error(
                format!("invalid inventory snapshot: {error}"),
                "Fix the JSON or restore the last valid checkpoint.",
            )
        })?;
        if snapshot.format != INVENTORY_SNAPSHOT_FORMAT {
            return Err(contract_error(
                format!("unsupported inventory format {:?}", snapshot.format),
                &format!("Use {INVENTORY_SNAPSHOT_FORMAT}; unknown major versions block."),
            ));
        }
        require_text(&snapshot.owner, "inventory owner")?;
        snapshot.inventory.validate(catalog)?;
        Ok(snapshot)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InteractionKind {
    Door { open: bool, locked: bool },
    Switch { active: bool },
    Checkpoint { respawn_mm: [i32; 3] },
    Pickup { item: String, amount: u32 },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct InteractionTarget {
    pub id: String,
    pub prompt: String,
    pub enabled: bool,
    #[serde(default)]
    pub required_item: Option<String>,
    pub emits: String,
    #[serde(flatten)]
    pub interaction: InteractionKind,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct InteractionOutcome {
    pub event: GameplayEvent,
    pub respawn_mm: Option<[i32; 3]>,
    pub pickup_remainder: u32,
}

impl InteractionTarget {
    pub fn validate(&self, catalog: &[ItemDefinition]) -> Result<()> {
        require_text(&self.id, "interaction id")?;
        require_text(&self.prompt, "interaction prompt")?;
        require_text(&self.emits, "interaction event")?;
        if self
            .required_item
            .as_ref()
            .is_some_and(|id| !catalog.iter().any(|definition| definition.id == *id))
        {
            return Err(contract_error(
                format!("interaction {:?} requires an unknown item", self.id),
                "Use an item id from the catalog.",
            ));
        }
        if matches!(self.interaction, InteractionKind::Pickup { amount: 0, .. }) {
            return Err(contract_error(
                format!("pickup interaction {:?} has zero items", self.id),
                "Set a positive pickup amount.",
            ));
        }
        if let InteractionKind::Pickup { item, .. } = &self.interaction {
            if !catalog.iter().any(|definition| definition.id == *item) {
                return Err(contract_error(
                    format!(
                        "pickup interaction {:?} references unknown item {item:?}",
                        self.id
                    ),
                    "Use an item id from the catalog.",
                ));
            }
        }
        Ok(())
    }

    /// Apply one interaction atomically to cloned target/inventory state.
    pub fn activate(
        &mut self,
        actor: &str,
        inventory: &mut InventoryState,
        catalog: &[ItemDefinition],
    ) -> Result<InteractionOutcome> {
        self.validate(catalog)?;
        inventory.validate(catalog)?;
        require_text(actor, "interaction actor")?;
        if !self.enabled {
            return Err(contract_error(
                format!("interaction {:?} is disabled", self.id),
                "Enable the target before interacting.",
            ));
        }
        if let Some(required) = &self.required_item {
            if !inventory.stacks.iter().any(|stack| stack.item == *required) {
                return Err(contract_error(
                    format!("interaction {:?} requires item {required:?}", self.id),
                    "Acquire the required item before interacting.",
                ));
            }
        }
        let mut next_target = self.clone();
        let mut next_inventory = inventory.clone();
        let mut respawn_mm = None;
        let mut pickup_remainder = 0;
        match &mut next_target.interaction {
            InteractionKind::Door { open, locked } => {
                if *locked && next_target.required_item.is_none() {
                    return Err(contract_error(
                        format!("door {:?} is locked without an unlock contract", self.id),
                        "Declare required_item or unlock the door through another typed event.",
                    ));
                }
                *locked = false;
                *open = true;
            }
            InteractionKind::Switch { active } => *active = !*active,
            InteractionKind::Checkpoint { respawn_mm: point } => respawn_mm = Some(*point),
            InteractionKind::Pickup { item, amount } => {
                pickup_remainder = next_inventory.add(item, *amount, catalog)?;
                *amount = pickup_remainder;
                if pickup_remainder == 0 {
                    next_target.enabled = false;
                }
            }
        }
        let event = GameplayEvent::Interacted {
            actor: actor.to_owned(),
            target: self.id.clone(),
            event: self.emits.clone(),
        };
        *self = next_target;
        *inventory = next_inventory;
        Ok(InteractionOutcome {
            event,
            respawn_mm,
            pickup_remainder,
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ObjectiveState {
    Inactive,
    Active,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct ObjectiveDefinition {
    pub id: String,
    pub description: String,
    pub required_events: BTreeMap<String, u32>,
    pub failure_events: Vec<String>,
    pub required: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct ObjectiveProgress {
    pub definition: ObjectiveDefinition,
    pub state: ObjectiveState,
    #[serde(default)]
    pub observed_events: BTreeMap<String, u32>,
}

impl ObjectiveProgress {
    pub fn validate(&self) -> Result<()> {
        require_text(&self.definition.id, "objective id")?;
        require_text(&self.definition.description, "objective description")?;
        if self.definition.required_events.is_empty()
            || self
                .definition
                .required_events
                .iter()
                .any(|(event, count)| event.trim().is_empty() || *count == 0)
        {
            return Err(contract_error(
                format!(
                    "objective {:?} has no concrete success evidence",
                    self.definition.id
                ),
                "Require at least one named event with a positive count.",
            ));
        }
        Ok(())
    }

    pub fn record_event(&mut self, event: &str) -> Result<Option<GameplayEvent>> {
        self.validate()?;
        if self.state != ObjectiveState::Active {
            return Ok(None);
        }
        if self
            .definition
            .failure_events
            .iter()
            .any(|name| name == event)
        {
            self.state = ObjectiveState::Failed;
        } else if self.definition.required_events.contains_key(event) {
            let count = self.observed_events.entry(event.to_owned()).or_default();
            *count = count.saturating_add(1);
            if self
                .definition
                .required_events
                .iter()
                .all(|(name, needed)| {
                    self.observed_events.get(name).copied().unwrap_or(0) >= *needed
                })
            {
                self.state = ObjectiveState::Completed;
            }
        }
        Ok(matches!(
            self.state,
            ObjectiveState::Completed | ObjectiveState::Failed
        )
        .then(|| GameplayEvent::ObjectiveChanged {
            objective: self.definition.id.clone(),
            state: self.state,
        }))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MatchState {
    Running,
    Won,
    Lost,
}

#[must_use]
pub fn match_state(objectives: &[ObjectiveProgress]) -> MatchState {
    if objectives
        .iter()
        .any(|objective| objective.definition.required && objective.state == ObjectiveState::Failed)
    {
        MatchState::Lost
    } else if objectives.iter().all(|objective| {
        !objective.definition.required || objective.state == ObjectiveState::Completed
    }) {
        MatchState::Won
    } else {
        MatchState::Running
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WeaponMode {
    Hitscan,
    Projectile { speed_mm_per_tick: u32 },
    Melee,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct AmmoContract {
    pub magazine_size: u32,
    pub reserve_capacity: u32,
    pub reload_ticks: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct WeaponAttachment {
    pub id: String,
    pub damage_modifier_bps: i32,
    pub spread_modifier_bps: i32,
    pub recoil_modifier_bps: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct WeaponDefinition {
    pub id: String,
    pub mode: WeaponMode,
    pub base_damage: u32,
    pub range_mm: u32,
    pub minimum_damage_bps: u32,
    pub spread_millidegrees: u32,
    pub recoil_millidegrees: u32,
    #[serde(default)]
    pub ammo: Option<AmmoContract>,
    #[serde(default)]
    pub attachments: Vec<WeaponAttachment>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct WeaponState {
    pub loaded: u32,
    pub reserve: u32,
    #[serde(default)]
    pub reload_complete_tick: Option<u64>,
    pub shot_sequence: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct WeaponShot {
    pub weapon: String,
    pub sequence: u64,
    pub damage: u32,
    pub spread_offset_millidegrees: [i32; 2],
    pub recoil_millidegrees: u32,
    pub projectile_speed_mm_per_tick: Option<u32>,
    pub events: Vec<GameplayEvent>,
}

impl WeaponDefinition {
    pub fn validate(&self) -> Result<()> {
        require_text(&self.id, "weapon id")?;
        if self.base_damage == 0 || self.range_mm == 0 || self.minimum_damage_bps > BASIS_POINTS_MAX
        {
            return Err(contract_error(
                format!("weapon {:?} damage/range/falloff is invalid", self.id),
                "Use positive damage/range and minimum_damage_bps within 0..=10,000.",
            ));
        }
        if matches!(
            self.mode,
            WeaponMode::Projectile {
                speed_mm_per_tick: 0
            }
        ) {
            return Err(contract_error(
                format!("projectile weapon {:?} has zero speed", self.id),
                "Use a positive fixed-tick projectile speed.",
            ));
        }
        if let Some(ammo) = &self.ammo {
            if ammo.magazine_size == 0 || ammo.reload_ticks == 0 {
                return Err(contract_error(
                    format!("weapon {:?} has an invalid ammo contract", self.id),
                    "Use a positive magazine size and reload duration.",
                ));
            }
        }
        let mut attachment_ids = BTreeSet::new();
        for attachment in &self.attachments {
            require_text(&attachment.id, "weapon attachment id")?;
            if !attachment_ids.insert(attachment.id.as_str()) {
                return Err(contract_error(
                    format!(
                        "weapon {:?} has duplicate attachment {:?}",
                        self.id, attachment.id
                    ),
                    "Attach each modifier once.",
                ));
            }
            if [
                attachment.damage_modifier_bps,
                attachment.spread_modifier_bps,
                attachment.recoil_modifier_bps,
            ]
            .into_iter()
            .any(|modifier| modifier.unsigned_abs() > ATTACHMENT_MODIFIER_LIMIT_BPS as u32)
            {
                return Err(contract_error(
                    format!(
                        "weapon attachment {:?} exceeds the modifier bound",
                        attachment.id
                    ),
                    "Keep every modifier within -10,000..=10,000 basis points.",
                ));
            }
        }
        Ok(())
    }

    pub fn fire(&self, state: &mut WeaponState, tick: u64, distance_mm: u32) -> Result<WeaponShot> {
        self.validate()?;
        if state
            .reload_complete_tick
            .is_some_and(|complete| tick < complete)
        {
            return Err(contract_error(
                format!("weapon {:?} is still reloading", self.id),
                "Wait for reload_complete_tick before firing.",
            ));
        }
        if let Some(ammo) = &self.ammo {
            if state.loaded > ammo.magazine_size || state.reserve > ammo.reserve_capacity {
                return Err(contract_error(
                    format!("weapon {:?} ammo state exceeds its contract", self.id),
                    "Clamp loaded/reserve ammo to the weapon definition.",
                ));
            }
            if state.loaded == 0 {
                return Err(contract_error(
                    format!("weapon {:?} magazine is empty", self.id),
                    "Reload or switch weapons.",
                ));
            }
            state.loaded -= 1;
        }
        state.reload_complete_tick = None;
        let sequence = state.shot_sequence;
        state.shot_sequence = state.shot_sequence.saturating_add(1);
        let damage = falloff_damage(
            apply_attachment_modifiers(
                self.base_damage,
                self.attachments
                    .iter()
                    .map(|attachment| attachment.damage_modifier_bps),
            ),
            self.minimum_damage_bps,
            self.range_mm,
            distance_mm,
        );
        let effective_spread = apply_attachment_modifiers(
            self.spread_millidegrees,
            self.attachments
                .iter()
                .map(|attachment| attachment.spread_modifier_bps),
        );
        let spread = deterministic_spread(&self.id, sequence, effective_spread);
        Ok(WeaponShot {
            weapon: self.id.clone(),
            sequence,
            damage,
            spread_offset_millidegrees: spread,
            recoil_millidegrees: apply_attachment_modifiers(
                self.recoil_millidegrees,
                self.attachments
                    .iter()
                    .map(|attachment| attachment.recoil_modifier_bps),
            ),
            projectile_speed_mm_per_tick: match self.mode {
                WeaponMode::Projectile { speed_mm_per_tick } => Some(speed_mm_per_tick),
                WeaponMode::Hitscan | WeaponMode::Melee => None,
            },
            events: vec![GameplayEvent::WeaponFired {
                weapon: self.id.clone(),
                sequence,
            }],
        })
    }

    pub fn start_reload(&self, state: &mut WeaponState, tick: u64) -> Result<()> {
        self.validate()?;
        let ammo = self.ammo.as_ref().ok_or_else(|| {
            contract_error(
                format!("weapon {:?} does not use ammunition", self.id),
                "Do not issue reload for this weapon mode.",
            )
        })?;
        if state.loaded >= ammo.magazine_size || state.reserve == 0 {
            return Err(contract_error(
                format!("weapon {:?} cannot reload in its current state", self.id),
                "Reload only a non-full magazine with reserve ammunition.",
            ));
        }
        state.reload_complete_tick = Some(tick.saturating_add(u64::from(ammo.reload_ticks)));
        Ok(())
    }

    pub fn finish_reload(&self, state: &mut WeaponState, tick: u64) -> Result<u32> {
        let ammo = self.ammo.as_ref().ok_or_else(|| {
            contract_error(
                format!("weapon {:?} does not use ammunition", self.id),
                "Do not issue reload for this weapon mode.",
            )
        })?;
        let complete = state.reload_complete_tick.ok_or_else(|| {
            contract_error(
                format!("weapon {:?} has no pending reload", self.id),
                "Start reload before finishing it.",
            )
        })?;
        if tick < complete {
            return Err(contract_error(
                format!("weapon {:?} reload has not completed", self.id),
                "Wait until reload_complete_tick.",
            ));
        }
        let needed = ammo.magazine_size.saturating_sub(state.loaded);
        let moved = needed.min(state.reserve);
        state.loaded += moved;
        state.reserve -= moved;
        state.reload_complete_tick = None;
        Ok(moved)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct GameplayContractDocument {
    pub format: String,
    #[serde(default)]
    pub items: Vec<ItemDefinition>,
    #[serde(default)]
    pub interactions: Vec<InteractionTarget>,
    #[serde(default)]
    pub objectives: Vec<ObjectiveDefinition>,
    #[serde(default)]
    pub weapons: Vec<WeaponDefinition>,
}

impl GameplayContractDocument {
    pub fn parse(text: &str) -> Result<Self> {
        let document: Self = serde_json::from_str(text).map_err(|error| {
            contract_error(
                format!("invalid gameplay contract: {error}"),
                "Fix the JSON and keep the supported format marker.",
            )
        })?;
        document.validate()?;
        Ok(document)
    }

    pub fn validate(&self) -> Result<()> {
        if self.format != GAMEPLAY_CONTRACT_FORMAT {
            return Err(contract_error(
                format!("unsupported gameplay contract format {:?}", self.format),
                &format!("Use {GAMEPLAY_CONTRACT_FORMAT}; unknown major versions block."),
            ));
        }
        validate_catalog(&self.items)?;
        unique_ids(
            self.interactions.iter().map(|value| value.id.as_str()),
            "interaction",
        )?;
        unique_ids(
            self.objectives.iter().map(|value| value.id.as_str()),
            "objective",
        )?;
        unique_ids(self.weapons.iter().map(|value| value.id.as_str()), "weapon")?;
        for interaction in &self.interactions {
            interaction.validate(&self.items)?;
        }
        for objective in &self.objectives {
            ObjectiveProgress {
                definition: objective.clone(),
                state: ObjectiveState::Inactive,
                observed_events: BTreeMap::new(),
            }
            .validate()?;
        }
        for weapon in &self.weapons {
            weapon.validate()?;
        }
        Ok(())
    }
}

fn validate_catalog(catalog: &[ItemDefinition]) -> Result<()> {
    unique_ids(catalog.iter().map(|item| item.id.as_str()), "item")?;
    for item in catalog {
        require_text(&item.id, "item id")?;
        require_text(&item.name, "item name")?;
        if item.max_stack == 0 {
            return Err(contract_error(
                format!("item {:?} has zero max_stack", item.id),
                "Use a positive stack limit.",
            ));
        }
    }
    Ok(())
}

fn unique_ids<'a>(values: impl Iterator<Item = &'a str>, label: &str) -> Result<()> {
    let mut ids = BTreeSet::new();
    for id in values {
        require_text(id, &format!("{label} id"))?;
        if !ids.insert(id) {
            return Err(contract_error(
                format!("duplicate {label} id {id:?}"),
                &format!("Give every {label} a stable unique id."),
            ));
        }
    }
    Ok(())
}

fn falloff_damage(base: u32, minimum_bps: u32, range_mm: u32, distance_mm: u32) -> u32 {
    if distance_mm > range_mm {
        return 0;
    }
    let minimum = u64::from(base) * u64::from(minimum_bps) / u64::from(BASIS_POINTS_MAX);
    let half = range_mm / 2;
    if distance_mm <= half || range_mm == half {
        return base;
    }
    let remaining = u64::from(range_mm - distance_mm);
    let falloff_span = u64::from(range_mm - half);
    let scaled = minimum
        + (u64::from(base).saturating_sub(minimum)).saturating_mul(remaining) / falloff_span;
    u32::try_from(scaled).unwrap_or(u32::MAX)
}

fn apply_attachment_modifiers(base: u32, modifiers: impl Iterator<Item = i32>) -> u32 {
    let modifier = modifiers.fold(0_i64, |total, value| total.saturating_add(i64::from(value)));
    let multiplier =
        (i64::from(BASIS_POINTS_MAX) + modifier).clamp(0, i64::from(BASIS_POINTS_MAX) * 2);
    let value = u64::from(base).saturating_mul(u64::try_from(multiplier).unwrap_or(0))
        / u64::from(BASIS_POINTS_MAX);
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn deterministic_spread(id: &str, sequence: u64, maximum: u32) -> [i32; 2] {
    if maximum == 0 {
        return [0, 0];
    }
    let mut hasher = blake3::Hasher::new();
    hasher.update(id.as_bytes());
    hasher.update(&sequence.to_le_bytes());
    let bytes = hasher.finalize();
    let data = bytes.as_bytes();
    let width = i64::from(maximum).saturating_mul(2).saturating_add(1);
    let x = i64::from(u16::from_le_bytes([data[0], data[1]])) % width - i64::from(maximum);
    let y = i64::from(u16::from_le_bytes([data[2], data[3]])) % width - i64::from(maximum);
    [x as i32, y as i32]
}

fn require_text(value: &str, label: &str) -> Result<()> {
    if value.trim().is_empty() {
        Err(contract_error(
            format!("{label} must not be empty"),
            &format!("Provide a stable {label}."),
        ))
    } else {
        Ok(())
    }
}

fn contract_error(message: String, hint: &str) -> EngineError {
    EngineError::Schema(message, Some(hint.to_owned()))
}
