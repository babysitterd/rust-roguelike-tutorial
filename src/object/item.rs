use crate::config::PLAYER;
use crate::game::map::is_out_of_bounds;
use crate::game::{render_all, Game, Tcod};
use crate::object::ai::Ai;
use crate::object::equipment::{Equipment, Slot};
use crate::object::Object;

use tcod::colors::*;
use tcod::input::{self, Event};

use serde::{Deserialize, Serialize};

const HEAL_AMOUNT: i32 = 40;
const LIGHTNING_DAMAGE: i32 = 40;
const LIGHTNING_RANGE: i32 = 5;
const CONFUSE_RANGE: i32 = 8;
const CONFUSE_NUM_TURNS: i32 = 10;
const FIREBALL_RADIUS: i32 = 3;
const FIREBALL_DAMAGE: i32 = 25;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Item {
    Heal,
    Lightning,
    Confusion,
    Fireball,
    Sword,
    Shield,
}

impl Item {
    pub fn create(item: Item, x: i32, y: i32) -> Object {
        match item {
            Item::Heal => {
                let mut object = Object::new(x, y, '!', VIOLET, "healing potion", false);
                object.item = Some(Item::Heal);
                object
            }
            Item::Shield => {
                let mut object = Object::new(x, y, 'o', DARKER_ORANGE, "shield", false);
                object.item = Some(Item::Shield);
                object.equipment = Some(Equipment {
                    equipped: false,
                    slot: Slot::LeftHand,
                    max_hp_bonus: 0,
                    defense_bonus: 1,
                    power_bonus: 0,
                });
                object
            }
            Item::Sword => {
                let mut object = Object::new(x, y, '/', SKY, "sword", false);
                object.item = Some(Item::Sword);
                object.equipment = Some(Equipment {
                    equipped: false,
                    slot: Slot::RightHand,
                    max_hp_bonus: 0,
                    defense_bonus: 0,
                    power_bonus: 3,
                });
                object
            }
            Item::Lightning => {
                let mut object =
                    Object::new(x, y, '#', LIGHT_YELLOW, "scroll of lightning bolt", false);
                object.item = Some(Item::Lightning);
                object
            }
            Item::Fireball => {
                // create a fireball scroll (10% chance)
                let mut object = Object::new(x, y, '#', LIGHT_YELLOW, "scroll of fireball", false);
                object.item = Some(Item::Fireball);
                object
            }
            Item::Confusion => {
                // create a confusion scroll (10% chance)
                let mut object = Object::new(x, y, '#', LIGHT_YELLOW, "scroll of confusion", false);
                object.item = Some(Item::Confusion);
                object
            }
        }
    }

    pub fn use_item(id: usize, tcod: &mut Tcod, game: &mut Game, objects: &mut [Object]) {
        use crate::object::item::Item::*;

        if let Some(item) = game.inventory[id].item {
            let on_use = match item {
                Heal => cast_heal,
                Lightning => cast_lightning,
                Confusion => cast_confusion,
                Fireball => cast_fireball,
                Sword => Equipment::toggle,
                Shield => Equipment::toggle,
            };
            match on_use(id, tcod, game, objects) {
                UseResult::UsedUp => {
                    game.inventory.remove(id);
                }
                UseResult::UsedAndKept => {}
                UseResult::Cancelled => {
                    game.messages.add("Cancelled", WHITE);
                }
            }
        } else {
            game.messages.add(
                format!("The {} can't be used.", game.inventory[id].name),
                WHITE,
            );
        }
    }
}

pub enum UseResult {
    UsedUp,
    Cancelled,
    UsedAndKept,
}

fn cast_heal(_id: usize, _tcod: &mut Tcod, game: &mut Game, objects: &mut [Object]) -> UseResult {
    let player = &mut objects[PLAYER];
    if let Some(fighter) = player.fighter {
        if fighter.hp == player.max_hp(game) {
            game.messages.add("You are already at full health", RED);
            return UseResult::Cancelled;
        }
        game.messages
            .add("Youre wounds start to feel better!", LIGHT_VIOLET);
        objects[PLAYER].heal(HEAL_AMOUNT, game);
        return UseResult::UsedUp;
    }
    UseResult::Cancelled
}

fn cast_lightning(
    _id: usize,
    tcod: &mut Tcod,
    game: &mut Game,
    objects: &mut [Object],
) -> UseResult {
    if let Some(id) = target_closest(tcod, objects, LIGHTNING_RANGE) {
        game.messages.add(
            format!(
                "A lightning bolt strikes the {} with a loud thunder! \
                                           The damage is {} git points.",
                objects[id].name, LIGHTNING_DAMAGE
            ),
            LIGHT_BLUE,
        );
        if let Some(xp) = objects[id].take_damage(LIGHTNING_DAMAGE, game) {
            objects[PLAYER].fighter.as_mut().unwrap().xp += xp;
        }
        UseResult::UsedUp
    } else {
        game.messages
            .add("No enemy is close enough to strike.", RED);
        UseResult::Cancelled
    }
}

fn cast_confusion(
    _id: usize,
    tcod: &mut Tcod,
    game: &mut Game,
    objects: &mut [Object],
) -> UseResult {
    game.messages.add(
        "Left-click an enemy to confuse it, or right-click to cancel.",
        LIGHT_CYAN,
    );
    if let Some(id) = target_monster(tcod, game, objects, Some(CONFUSE_RANGE as f32)) {
        game.messages.add(
            format!(
                "The eyes of {} look vacant, as he starts to stumble around!",
                objects[id].name
            ),
            LIGHT_GREEN,
        );
        let old_ai = objects[id].ai.take().unwrap_or(Ai::Basic);
        objects[id].ai = Some(Ai::Confused {
            previous_ai: Box::new(old_ai),
            lasts_for: CONFUSE_NUM_TURNS,
        });
        UseResult::UsedUp
    } else {
        UseResult::Cancelled
    }
}

fn cast_fireball(
    _id: usize,
    tcod: &mut Tcod,
    game: &mut Game,
    objects: &mut [Object],
) -> UseResult {
    game.messages.add(
        "Left-click a target tile for the fireball, or right-click to cancel.",
        LIGHT_CYAN,
    );
    let (x, y) = match target_tile(tcod, game, objects, None) {
        Some(pos) => pos,
        None => return UseResult::Cancelled,
    };
    game.messages.add(
        format!(
            "The fireball explodes, burning everything within {} tiles!",
            FIREBALL_RADIUS
        ),
        ORANGE,
    );

    let mut xp_to_gain = 0;
    for (id, obj) in objects.iter_mut().enumerate() {
        if obj.distance(x, y) <= FIREBALL_RADIUS as f32 && obj.fighter.is_some() {
            game.messages.add(
                format!(
                    "The {} gets burned for {} hit points.",
                    obj.name, FIREBALL_DAMAGE
                ),
                ORANGE,
            );
            if let Some(xp) = obj.take_damage(FIREBALL_DAMAGE, game) {
                // Not getting any xp for commiting suicide
                if id != PLAYER {
                    xp_to_gain += xp;
                }
            }
        }
    }
    objects[PLAYER].fighter.as_mut().unwrap().xp += xp_to_gain;

    UseResult::UsedUp
}

fn target_closest(tcod: &Tcod, objects: &[Object], max_range: i32) -> Option<usize> {
    let mut closest_enemy = None;
    let mut closest_distance = (max_range + 1) as f32;
    for (id, object) in objects.iter().enumerate() {
        if id != PLAYER
            && object.fighter.is_some()
            && object.ai.is_some()
            && tcod.fov.is_in_fov(object.x, object.y)
        {
            let dist = objects[PLAYER].distance_to(object);
            if dist < closest_distance {
                closest_enemy = Some(id);
                closest_distance = dist;
            }
        }
    }
    closest_enemy
}

/// return the position of a tile left-clicked in player's FOV (optionally in a
/// range), or (None,None) if right-clicked.
fn target_tile(
    tcod: &mut Tcod,
    game: &mut Game,
    objects: &[Object],
    max_range: Option<f32>,
) -> Option<(i32, i32)> {
    use tcod::input::KeyCode::Escape;
    loop {
        // render the screen. this erases the inventory and shows the names of
        // objects under the mouse.
        tcod.root.flush();
        let event = input::check_for_event(input::KEY_PRESS | input::MOUSE).map(|e| e.1);
        match event {
            Some(Event::Mouse(m)) => tcod.mouse = m,
            Some(Event::Key(k)) => tcod.key = k,
            None => tcod.key = Default::default(),
        }
        render_all(tcod, game, objects);

        let (x, y) = (tcod.mouse.cx as i32, tcod.mouse.cy as i32);

        let in_fov = !is_out_of_bounds(x, y) && tcod.fov.is_in_fov(x, y);
        let in_range = max_range.map_or(true, |r| objects[PLAYER].distance(x, y) <= r);
        if tcod.mouse.lbutton_pressed && in_fov && in_range {
            return Some((x, y));
        }

        if tcod.mouse.rbutton_pressed || tcod.key.code == Escape {
            return None;
        }
    }
}

fn target_monster(
    tcod: &mut Tcod,
    game: &mut Game,
    objects: &[Object],
    max_range: Option<f32>,
) -> Option<usize> {
    loop {
        match target_tile(tcod, game, objects, max_range) {
            Some((x, y)) => {
                // return the first clicked monster, otherwise continue looping
                for (id, obj) in objects.iter().enumerate() {
                    if obj.pos() == (x, y) && obj.fighter.is_some() && id != PLAYER {
                        return Some(id);
                    }
                }
            }
            None => return None,
        }
    }
}
