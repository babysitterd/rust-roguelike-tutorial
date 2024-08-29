pub mod ai;
pub mod equipment;
pub mod fighter;
pub mod item;
pub mod monster;

use ai::Ai;
use equipment::Equipment;
use fighter::Fighter;
use item::Item;

use crate::game::Game;
use crate::game::Messages;

use tcod::colors::*;
use tcod::console::*;

use std::cmp;

use serde::{Deserialize, Serialize};

// experience and level-ups
const LEVEL_UP_BASE: i32 = 200;
const LEVEL_UP_FACTOR: i32 = 150;

/// This is a generic object: the player, a monster, an item, the stairs...
/// It's always represented by a character on screen.
#[derive(Debug, Serialize, Deserialize)]
pub struct Object {
    pub x: i32,
    pub y: i32,
    pub glyph: char,
    pub color: Color,
    pub name: String,
    pub blocks: bool,
    pub alive: bool,
    pub level: i32,
    pub fighter: Option<Fighter>,
    pub ai: Option<Ai>,
    pub item: Option<Item>,
    pub equipment: Option<Equipment>,
    pub always_visible: bool,
}

impl Object {
    pub fn new(x: i32, y: i32, glyph: char, color: Color, name: &str, blocks: bool) -> Self {
        Object {
            x: x,
            y: y,
            glyph: glyph,
            color: color,
            name: name.into(),
            blocks: blocks,
            alive: false,
            level: 1,
            fighter: None,
            ai: None,
            item: None,
            equipment: None,
            always_visible: false,
        }
    }

    pub fn take_damage(&mut self, damage: i32, game: &mut Game) -> Option<i32> {
        if let Some(fighter) = self.fighter.as_mut() {
            if damage > 0 {
                fighter.hp = cmp::max(fighter.hp - damage, 0);
            }
        }

        if let Some(fighter) = self.fighter {
            if self.alive && fighter.hp == 0 {
                self.alive = false;
                fighter.on_death.callback(self, game);
                return Some(fighter.xp);
            }
        }

        None
    }

    pub fn attack(&mut self, other: &mut Object, game: &mut Game) {
        let damage = self.power(game) - other.defense(game);
        if damage > 0 {
            game.messages.add(
                format!(
                    "{} attacks {} for {} hit points.",
                    self.name, other.name, damage
                ),
                WHITE,
            );
            if let Some(xp) = other.take_damage(damage, game) {
                self.fighter.as_mut().unwrap().xp += xp;
            }
        } else {
            game.messages.add(
                format!("{} attacks {} but it has no effect!", self.name, other.name),
                WHITE,
            );
        }
    }

    pub fn heal(&mut self, amount: i32, game: &Game) {
        let max_hp = self.max_hp(game);
        if let Some(ref mut fighter) = self.fighter {
            fighter.hp = cmp::min(fighter.hp + amount, max_hp);
        }
    }

    pub fn max_hp(&self, game: &Game) -> i32 {
        let base_max_hp = self.fighter.map_or(0, |f| f.base_max_hp);
        let bonus: i32 = self
            .get_all_equipped(game)
            .iter()
            .map(|e| e.max_hp_bonus)
            .sum();
        base_max_hp + bonus
    }

    pub fn power(&self, game: &Game) -> i32 {
        let base_power = self.fighter.map_or(0, |f| f.base_power);
        let bonus: i32 = self
            .get_all_equipped(game)
            .iter()
            .map(|e| e.power_bonus)
            .sum();
        base_power + bonus
    }

    pub fn defense(&self, game: &Game) -> i32 {
        let base_defense = self.fighter.map_or(0, |f| f.base_defense);
        let bonus: i32 = self
            .get_all_equipped(game)
            .iter()
            .map(|e| e.defense_bonus)
            .sum();
        base_defense + bonus
    }

    pub fn get_all_equipped(&self, game: &Game) -> Vec<Equipment> {
        if self.name == "player" {
            game.inventory
                .iter()
                .filter(|item| item.equipment.map_or(false, |e| e.equipped))
                .map(|item| item.equipment.unwrap())
                .collect()
        } else {
            vec![]
        }
    }

    pub fn equip(&mut self, messages: &mut Messages) {
        if self.item.is_none() {
            messages.add(
                format!("Can't equip {:?} because it's not an Item.", self),
                RED,
            );
            return;
        }
        if let Some(ref mut equipment) = self.equipment {
            if !equipment.equipped {
                equipment.equipped = true;
                messages.add(
                    format!("Equipped {} on {}.", self.name, equipment.slot),
                    LIGHT_GREEN,
                );
            }
        } else {
            messages.add(
                format!("Can't equip {:?} because it's not an Equipment.", self),
                RED,
            );
        }
    }

    pub fn dequip(&mut self, messages: &mut Messages) {
        if self.item.is_none() {
            messages.add(
                format!("Can't dequip {:?} because it's not an Item.", self),
                RED,
            );
            return;
        }
        if let Some(ref mut equipment) = self.equipment {
            if equipment.equipped {
                equipment.equipped = false;
                messages.add(
                    format!("Dequipped {} on {}.", self.name, equipment.slot),
                    LIGHT_GREEN,
                );
            }
        } else {
            messages.add(
                format!("Can't dequip {:?} because it's not an Equipment.", self),
                RED,
            );
        }
    }

    pub fn level_up_xp(&self) -> i32 {
        LEVEL_UP_BASE + self.level * LEVEL_UP_FACTOR
    }

    /// set the color and then draw the character that represents this object at its position
    pub fn draw(&self, con: &mut dyn Console) {
        con.set_default_foreground(self.color);
        con.put_char(self.x, self.y, self.glyph, BackgroundFlag::None);
    }

    pub fn pos(&self) -> (i32, i32) {
        return (self.x, self.y);
    }

    pub fn set_pos(&mut self, x: i32, y: i32) {
        self.x = x;
        self.y = y;
    }

    pub fn distance_to(&self, other: &Object) -> f32 {
        self.distance(other.x, other.y)
    }

    pub fn distance(&self, x: i32, y: i32) -> f32 {
        let dx = x - self.x;
        let dy = y - self.y;
        ((dx * dx + dy * dy) as f32).sqrt()
    }
}
