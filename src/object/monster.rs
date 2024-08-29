use crate::object::ai::Ai;
use crate::object::fighter::{DeathCallback, Fighter};
use crate::object::Object;

use tcod::colors::*;

#[derive(Clone, Copy, Debug)]
pub enum Monster {
    Orc,
    Troll,
}

impl Monster {
    pub fn create(monster: Monster, x: i32, y: i32) -> Object {
        match monster {
            Monster::Orc => create_orc(x, y),
            Monster::Troll => create_troll(x, y),
        }
    }
}

fn create_orc(x: i32, y: i32) -> Object {
    let mut orc = Object::new(x, y, 'o', DESATURATED_GREEN, "orc", true);
    orc.alive = true;
    orc.fighter = Some(Fighter {
        base_max_hp: 20,
        hp: 20,
        base_defense: 0,
        base_power: 4,
        xp: 35,
        on_death: DeathCallback::Monster,
    });
    orc.ai = Some(Ai::Basic);
    orc
}

fn create_troll(x: i32, y: i32) -> Object {
    let mut troll = Object::new(x, y, 'T', DARKER_GREEN, "troll", true);
    troll.alive = true;
    troll.fighter = Some(Fighter {
        base_max_hp: 30,
        hp: 30,
        base_defense: 2,
        base_power: 8,
        xp: 100,
        on_death: DeathCallback::Monster,
    });
    troll.ai = Some(Ai::Basic);
    troll
}
