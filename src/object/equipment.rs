use crate::game::{Game, Tcod};
use crate::object::item::UseResult;
use crate::object::Object;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Slot {
    LeftHand,
    RightHand,
    Head,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Equipment {
    pub slot: Slot,
    pub equipped: bool,
    pub max_hp_bonus: i32,
    pub defense_bonus: i32,
    pub power_bonus: i32,
}

impl std::fmt::Display for Slot {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Slot::Head => write!(f, "head"),
            Slot::LeftHand => write!(f, "left hand"),
            Slot::RightHand => write!(f, "right hand"),
        }
    }
}

impl Equipment {
    pub fn toggle(
        id: usize,
        _tcod: &mut Tcod,
        game: &mut Game,
        _objects: &mut [Object],
    ) -> UseResult {
        let equipment = match game.inventory[id].equipment {
            Some(equipment) => equipment,
            None => return UseResult::Cancelled,
        };

        if equipment.equipped {
            game.inventory[id].dequip(&mut game.messages);
        } else {
            if let Some(current) = Self::get_equipped_in_slot(equipment.slot, &game.inventory) {
                game.inventory[current].dequip(&mut game.messages);
            }
            game.inventory[id].equip(&mut game.messages);
        }

        UseResult::UsedAndKept
    }

    pub fn get_equipped_in_slot(slot: Slot, inventory: &[Object]) -> Option<usize> {
        for (id, item) in inventory.iter().enumerate() {
            if item
                .equipment
                .as_ref()
                .map_or(false, |e| e.equipped && e.slot == slot)
            {
                return Some(id);
            }
        }
        None
    }
}
