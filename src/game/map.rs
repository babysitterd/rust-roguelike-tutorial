use crate::game::PLAYER;
use crate::object::item::Item;
use crate::object::monster::Monster;
use crate::object::Object;

use tcod::colors::*;

use rand::distributions::{IndependentSample, Weighted, WeightedChoice};
use rand::Rng;
use std::cmp;

use serde::{Deserialize, Serialize};

// size of the map
pub const MAP_WIDTH: i32 = 80;
pub const MAP_HEIGHT: i32 = 43;

// parameters for dungeon generator
const ROOM_MAX_SIZE: i32 = 10;
const ROOM_MIN_SIZE: i32 = 6;
const MAX_ROOMS: i32 = 30;

pub type Map = Vec<Vec<Tile>>;

/// A tile of the map and its properties
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Tile {
    pub blocked: bool,
    pub explored: bool,
    pub block_sight: bool,
}

pub fn is_out_of_bounds(x: i32, y: i32) -> bool {
    x < 0 || x >= MAP_WIDTH || y < 0 || y >= MAP_HEIGHT
}

pub fn is_blocked(x: i32, y: i32, map: &Map, objects: &[Object]) -> bool {
    if map[x as usize][y as usize].blocked {
        return true;
    }

    objects
        .iter()
        .any(|object| object.pos() == (x, y) && object.blocks)
}

pub fn make_map(objects: &mut Vec<Object>, level: u32) -> Map {
    let mut map = vec![vec![Tile::wall(); MAP_HEIGHT as usize]; MAP_WIDTH as usize];

    let mut rooms: Vec<Rectangle> = vec![];

    for _ in 0..MAX_ROOMS {
        // random width and height
        let w = rand::thread_rng().gen_range(ROOM_MIN_SIZE, ROOM_MAX_SIZE + 1);
        let h = rand::thread_rng().gen_range(ROOM_MIN_SIZE, ROOM_MAX_SIZE + 1);
        // random position without going out of the boundaries of the map
        let x = rand::thread_rng().gen_range(0, MAP_WIDTH - w);
        let y = rand::thread_rng().gen_range(0, MAP_HEIGHT - h);

        let new_room = Rectangle::new(x, y, w, h);

        let failed = rooms.iter().any(|other| new_room.intersects(other));

        if !failed {
            let (new_x, new_y) = new_room.center();

            if let Some(prev) = rooms.last() {
                let (prev_x, prev_y) = prev.center();

                if rand::random() {
                    carve_h_tunnel(prev_x, new_x, prev_y, &mut map);
                    carve_v_tunnel(prev_y, new_y, new_x, &mut map);
                } else {
                    carve_v_tunnel(prev_y, new_y, prev_x, &mut map);
                    carve_h_tunnel(prev_x, new_x, new_y, &mut map);
                }
            } else {
                let player = &mut objects[PLAYER];
                player.set_pos(new_x, new_y);
            }

            rooms.push(Rectangle::new(x, y, w, h));
        }
    }

    // fresh start: clean up all everything except for the player
    objects.truncate(1);

    for room in &rooms {
        carve_room(room, &mut map);
        fill_with_objects(&room, &map, objects, level);
    }

    // stairs to go one level deeper
    let last_room = rooms[rooms.len() - 1].center();
    let mut stairs = Object::new(last_room.0, last_room.1, '<', WHITE, "stairs", false);
    stairs.always_visible = true;
    objects.push(stairs);

    map
}

fn carve_room(room: &Rectangle, map: &mut Map) {
    for i in (room.x1 + 1)..room.x2 {
        for j in (room.y1 + 1)..room.y2 {
            map[i as usize][j as usize] = Tile::empty();
        }
    }
}

fn carve_h_tunnel(x1: i32, x2: i32, y: i32, map: &mut Map) {
    let from = cmp::min(x1, x2);
    let to = cmp::max(x1, x2) + 1;
    for x in from..to {
        map[x as usize][y as usize] = Tile::empty();
    }
}

fn carve_v_tunnel(y1: i32, y2: i32, x: i32, map: &mut Map) {
    let from = cmp::min(y1, y2);
    let to = cmp::max(y1, y2) + 1;
    for y in from..to {
        map[x as usize][y as usize] = Tile::empty();
    }
}

impl Tile {
    pub fn empty() -> Self {
        Tile {
            blocked: false,
            explored: false,
            block_sight: false,
        }
    }

    pub fn wall() -> Self {
        Tile {
            blocked: true,
            explored: false,
            block_sight: true,
        }
    }
}

struct Rectangle {
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
}

impl Rectangle {
    fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Rectangle {
            x1: x,
            y1: y,
            x2: x + w,
            y2: y + h,
        }
    }

    fn center(&self) -> (i32, i32) {
        ((self.x1 + self.x2) / 2, (self.y1 + self.y2) / 2)
    }

    fn intersects(&self, other: &Rectangle) -> bool {
        self.x1 <= other.x2 && self.x2 >= other.x1 && self.y1 <= other.y2 && self.y2 >= other.y1
    }
}

struct Transition {
    level: u32,
    value: u32,
}

fn from_dungeon_level(table: &[Transition], level: u32) -> u32 {
    table
        .iter()
        .rev()
        .find(|transition| level >= transition.level)
        .map_or(0, |transition| transition.value)
}

fn fill_with_objects(room: &Rectangle, map: &Map, objects: &mut Vec<Object>, level: u32) {
    let max_monsters = from_dungeon_level(
        &[
            Transition { level: 1, value: 2 },
            Transition { level: 4, value: 3 },
            Transition { level: 6, value: 5 },
        ],
        level,
    );

    let troll_chance = from_dungeon_level(
        &[
            Transition {
                level: 3,
                value: 15,
            },
            Transition {
                level: 5,
                value: 30,
            },
            Transition {
                level: 7,
                value: 60,
            },
        ],
        level,
    );

    let mut monster_chances = [
        Weighted {
            weight: 80,
            item: Monster::Orc,
        },
        Weighted {
            weight: troll_chance,
            item: Monster::Troll,
        },
    ];
    let monster_choice = WeightedChoice::new(&mut monster_chances);

    let num_monsters = rand::thread_rng().gen_range(0, max_monsters + 1);
    for _ in 0..num_monsters {
        let x = rand::thread_rng().gen_range(room.x1 + 1, room.x2);
        let y = rand::thread_rng().gen_range(room.y1 + 1, room.y2);

        if is_blocked(x, y, map, objects) {
            continue;
        }

        let monster = Monster::create(monster_choice.ind_sample(&mut rand::thread_rng()), x, y);
        objects.push(monster);
    }

    let max_items = from_dungeon_level(
        &[
            Transition { level: 1, value: 1 },
            Transition { level: 4, value: 2 },
        ],
        level,
    );

    let mut item_chances = [
        Weighted {
            weight: 35,
            item: Item::Heal,
        },
        Weighted {
            weight: from_dungeon_level(&[Transition { level: 4, value: 5 }], level),
            item: Item::Sword,
        },
        Weighted {
            weight: from_dungeon_level(
                &[Transition {
                    level: 8,
                    value: 15,
                }],
                level,
            ),
            item: Item::Shield,
        },
        Weighted {
            weight: from_dungeon_level(
                &[Transition {
                    level: 4,
                    value: 25,
                }],
                level,
            ),
            item: Item::Lightning,
        },
        Weighted {
            weight: from_dungeon_level(
                &[Transition {
                    level: 6,
                    value: 25,
                }],
                level,
            ),
            item: Item::Fireball,
        },
        Weighted {
            weight: from_dungeon_level(
                &[Transition {
                    level: 2,
                    value: 10,
                }],
                level,
            ),
            item: Item::Confusion,
        },
    ];
    let item_choice = WeightedChoice::new(&mut item_chances);

    let num_items = rand::thread_rng().gen_range(0, max_items + 1);
    for _ in 0..num_items {
        let x = rand::thread_rng().gen_range(room.x1 + 1, room.x2);
        let y = rand::thread_rng().gen_range(room.y1 + 1, room.y2);

        if is_blocked(x, y, map, objects) {
            continue;
        }

        let mut item = Item::create(item_choice.ind_sample(&mut rand::thread_rng()), x, y);
        item.always_visible = true;
        objects.push(item);
    }
}
