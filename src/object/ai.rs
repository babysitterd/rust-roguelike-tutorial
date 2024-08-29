use crate::config::PLAYER;
use crate::game::{move_by, Game, Tcod};
use crate::object::Object;

use tcod::colors::*;

use rand::Rng;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Ai {
    Basic,
    Confused {
        previous_ai: Box<Ai>,
        lasts_for: i32,
    },
}

pub fn ai_take_turn(monster_id: usize, tcod: &Tcod, game: &mut Game, objects: &mut [Object]) {
    use Ai::*;
    if let Some(ai) = objects[monster_id].ai.take() {
        let new_ai = match ai {
            Basic => ai_basic(monster_id, tcod, game, objects),
            Confused {
                previous_ai,
                lasts_for,
            } => ai_confused(monster_id, tcod, game, objects, previous_ai, lasts_for),
        };
        objects[monster_id].ai = Some(new_ai);
    }
}

pub fn ai_basic(monster_id: usize, tcod: &Tcod, game: &mut Game, objects: &mut [Object]) -> Ai {
    let (monster_x, monster_y) = objects[monster_id].pos();
    if tcod.fov.is_in_fov(monster_x, monster_y) {
        if objects[monster_id].distance_to(&objects[PLAYER]) >= 2.0 {
            let (player_x, player_y) = objects[PLAYER].pos();
            move_towards(monster_id, player_x, player_y, game, objects);
        } else if objects[PLAYER].fighter.map_or(false, |f| f.hp >= 0) {
            let (player, monster) = mut_two(PLAYER, monster_id, objects);
            monster.attack(player, game);
        }
    }
    Ai::Basic
}

pub fn ai_confused(
    monster_id: usize,
    _tcod: &Tcod,
    game: &mut Game,
    objects: &mut [Object],
    previous_ai: Box<Ai>,
    lasts_for: i32,
) -> Ai {
    if lasts_for >= 0 {
        // still confused
        move_by(
            monster_id,
            rand::thread_rng().gen_range(-1, 2),
            rand::thread_rng().gen_range(-1, 2),
            game,
            objects,
        );
        Ai::Confused {
            previous_ai: previous_ai,
            lasts_for: lasts_for - 1,
        }
    } else {
        game.messages.add(
            format!("The {} is no longer confused!", objects[monster_id].name),
            RED,
        );
        *previous_ai
    }
}

fn move_towards(id: usize, target_x: i32, target_y: i32, game: &mut Game, objects: &mut [Object]) {
    let dx = target_x - objects[id].x;
    let dy = target_y - objects[id].y;
    let distance = ((dx * dx + dy * dy) as f32).sqrt();

    // normalize it to length 1 (preserving direction), then round it and
    // convert to integer so the movement is restricted to the map grid
    let dx = (dx as f32 / distance).round() as i32;
    let dy = (dy as f32 / distance).round() as i32;
    move_by(id, dx, dy, game, objects);
}

/// Mutably borrow two *separate* elements from the slice
fn mut_two<T>(first_id: usize, second_id: usize, items: &mut [T]) -> (&mut T, &mut T) {
    assert!(first_id < second_id);
    let (first_slice, second_slice) = items.split_at_mut(second_id);
    (&mut first_slice[first_id], &mut second_slice[0])
}
