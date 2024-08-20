use rand::Rng;
use std::cmp;
use std::error::Error;

use std::fs::File;
use std::io::{Read, Write};

use serde::{Deserialize, Serialize};

use tcod::colors::*;
use tcod::console::*;

use tcod::input::{self, Event, Key, Mouse};
use tcod::map::{FovAlgorithm, Map as FovMap};

const SCREEN_WIDTH: i32 = 80;
const SCREEN_HEIGHT: i32 = 50;

const LIMIT_FPS: i32 = 20;

const SAVEGAME_FILE: &str = "savegame.dat";

// size of the map
const MAP_WIDTH: i32 = 80;
const MAP_HEIGHT: i32 = 43;

// sizes and coordinates relevant for the GUI
const BAR_WIDTH: i32 = 20;
const PANEL_HEIGHT: i32 = 7;
const PANEL_Y: i32 = SCREEN_HEIGHT - PANEL_HEIGHT;
const INVENTORY_WIDTH: i32 = 50;
const LEVEL_SCREEN_WIDTH: i32 = 40;
const CHARACTER_SCREEN_WIDTH: i32 = 30;

const MSG_X: i32 = BAR_WIDTH + 2;
const MSG_WIDTH: i32 = SCREEN_WIDTH - BAR_WIDTH - 2;
const MSG_HEIGHT: usize = PANEL_HEIGHT as usize - 1;

// parameters for dungeon generator
const ROOM_MAX_SIZE: i32 = 10;
const ROOM_MIN_SIZE: i32 = 6;
const MAX_ROOMS: i32 = 30;
const MAX_ROOM_ITEMS: i32 = 2;

const FOV_ALGO: FovAlgorithm = FovAlgorithm::Basic; // default FOV algorithm
const FOV_LIGHT_WALLS: bool = true; // light walls or not
const TORCH_RADIUS: i32 = 10;
const HEAL_AMOUNT: i32 = 4;
const LIGHTNING_DAMAGE: i32 = 40;
const LIGHTNING_RANGE: i32 = 5;
const CONFUSE_RANGE: i32 = 8;
const CONFUSE_NUM_TURNS: i32 = 10;
const FIREBALL_RADIUS: i32 = 3;
const FIREBALL_DAMAGE: i32 = 12;

// experience and level-ups
const LEVEL_UP_BASE: i32 = 200;
const LEVEL_UP_FACTOR: i32 = 150;

const COLOR_DARK_WALL: Color = Color { r: 0, g: 0, b: 100 };
const COLOR_LIGHT_WALL: Color = Color {
    r: 130,
    g: 110,
    b: 50,
};
const COLOR_DARK_GROUND: Color = Color {
    r: 50,
    g: 50,
    b: 150,
};
const COLOR_LIGHT_GROUND: Color = Color {
    r: 200,
    g: 180,
    b: 50,
};

const MAX_ROOM_MONSTERS: i32 = 3;

// player will always be the first object
const PLAYER: usize = 0;

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

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
enum DeathCallback {
    Player,
    Monster,
}

impl DeathCallback {
    fn callback(self, object: &mut Object, game: &mut Game) {
        use DeathCallback::*;
        let callback: fn(&mut Object, &mut Game) = match self {
            Player => player_death,
            Monster => monster_death,
        };
        callback(object, game);
    }
}

fn player_death(player: &mut Object, game: &mut Game) {
    game.messages.add("You died!", RED);

    player.glyph = '%';
    player.color = DARK_RED;
}

fn monster_death(monster: &mut Object, game: &mut Game) {
    game.messages.add(
        format!(
            "{} is dead! You gain {} experience points.",
            monster.name,
            monster.fighter.unwrap().xp
        ),
        ORANGE,
    );

    monster.glyph = '%';
    monster.color = DARK_RED;
    monster.blocks = false;
    monster.fighter = None;
    monster.ai = None;
    monster.name = format!("remains of {}", monster.name);
}

// combat-related properties and methods (monster, player, NPC).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
struct Fighter {
    max_hp: i32,
    hp: i32,
    defense: i32,
    power: i32,
    xp: i32,
    on_death: DeathCallback,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum Ai {
    Basic,
    Confused {
        previous_ai: Box<Ai>,
        lasts_for: i32,
    },
}

#[derive(Serialize, Deserialize)]
struct Messages {
    messages: Vec<(String, Color)>,
}

impl Messages {
    pub fn new() -> Self {
        Self { messages: vec![] }
    }

    pub fn add<T: Into<String>>(&mut self, message: T, color: Color) {
        self.messages.push((message.into(), color));
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &(String, Color)> {
        self.messages.iter()
    }
}

/// This is a generic object: the player, a monster, an item, the stairs...
/// It's always represented by a character on screen.
#[derive(Debug, Serialize, Deserialize)]
struct Object {
    x: i32,
    y: i32,
    glyph: char,
    color: Color,
    name: String,
    blocks: bool,
    alive: bool,
    level: i32,
    fighter: Option<Fighter>,
    ai: Option<Ai>,
    item: Option<Item>,
    always_visible: bool,
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
        if let (Some(off), Some(def)) = (self.fighter, other.fighter) {
            let damage = off.power - def.defense;
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
    }

    pub fn heal(&mut self, amount: i32) {
        if let Some(ref mut fighter) = self.fighter {
            fighter.hp = cmp::min(fighter.hp + amount, fighter.max_hp);
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

/// move by the given amount, if the destination is not blocked
fn move_by(id: usize, dx: i32, dy: i32, game: &mut Game, objects: &mut [Object]) {
    let pos = objects[id].pos();

    let new_x = pos.0 + dx;
    let new_y = pos.1 + dy;

    if is_blocked(new_x, new_y, &game.map, objects) || is_out_of_bounds(new_x, new_y) {
        return;
    }

    objects[id].set_pos(new_x, new_y);
}

fn player_move_or_attack(dx: i32, dy: i32, game: &mut Game, objects: &mut [Object]) {
    let pos = objects[PLAYER].pos();
    let new_pos = (pos.0 + dx, pos.1 + dy);

    let target_id = objects
        .iter()
        .position(|enemy| enemy.fighter.is_some() && enemy.pos() == new_pos);

    if let Some(id) = target_id {
        let (player, monster) = mut_two(PLAYER, id, objects);
        player.attack(monster, game);
    } else {
        move_by(PLAYER, dx, dy, game, objects);
    }
}

/// Mutably borrow two *separate* elements from the slice
fn mut_two<T>(first_id: usize, second_id: usize, items: &mut [T]) -> (&mut T, &mut T) {
    assert!(first_id < second_id);
    let (first_slice, second_slice) = items.split_at_mut(second_id);
    (&mut first_slice[first_id], &mut second_slice[0])
}

fn ai_take_turn(monster_id: usize, tcod: &Tcod, game: &mut Game, objects: &mut [Object]) {
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

fn ai_basic(monster_id: usize, tcod: &Tcod, game: &mut Game, objects: &mut [Object]) -> Ai {
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

fn ai_confused(
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

/// A tile of the map and its properties
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct Tile {
    blocked: bool,
    explored: bool,
    block_sight: bool,
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

type Map = Vec<Vec<Tile>>;

#[derive(Serialize, Deserialize)]
struct Game {
    map: Map,
    messages: Messages,
    inventory: Vec<Object>,
    dungeon_level: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PlayerAction {
    TookTurn,
    DidntTakeTurn,
    Exit,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
enum Item {
    Heal,
    Lightning,
    Confusion,
    Fireball,
}

enum UseResult {
    UsedUp,
    Cancelled,
}

fn cast_heal(_id: usize, _tcod: &mut Tcod, game: &mut Game, objects: &mut [Object]) -> UseResult {
    if let Some(fighter) = objects[PLAYER].fighter {
        if fighter.hp == fighter.max_hp {
            game.messages.add("You are already at full health", RED);
            return UseResult::Cancelled;
        }
        game.messages
            .add("Youre wounds start to feel better!", LIGHT_VIOLET);
        objects[PLAYER].heal(HEAL_AMOUNT);
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

fn pick_item_up(id: usize, game: &mut Game, objects: &mut Vec<Object>) {
    if game.inventory.len() >= 26 {
        game.messages.add(
            format!("Your inventory is full, can't pick up {}", objects[id].name),
            RED,
        );
    } else {
        let item = objects.swap_remove(id);
        game.messages
            .add(format!("You've just picked up a {}!", item.name), GREEN);
        game.inventory.push(item);
    }
}

fn drop_item(id: usize, game: &mut Game, objects: &mut Vec<Object>) {
    let mut item = game.inventory.remove(id);
    item.set_pos(objects[PLAYER].x, objects[PLAYER].y);
    game.messages
        .add(format!("You dropped a {}.", item.name), YELLOW);
    objects.push(item);
}

fn use_item(id: usize, tcod: &mut Tcod, game: &mut Game, objects: &mut [Object]) {
    use Item::*;

    if let Some(item) = game.inventory[id].item {
        let on_use = match item {
            Heal => cast_heal,
            Lightning => cast_lightning,
            Confusion => cast_confusion,
            Fireball => cast_fireball,
        };
        match on_use(id, tcod, game, objects) {
            UseResult::UsedUp => {
                game.inventory.remove(id);
            }
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

fn level_up(tcod: &mut Tcod, game: &mut Game, objects: &mut [Object]) {
    let player = &mut objects[PLAYER];
    let level_up_xp = player.level_up_xp();

    if player.fighter.as_ref().map_or(0, |f| f.xp) >= level_up_xp {
        player.level += 1;
        game.messages.add(
            format!(
                "Your battle skills grow stronger! You reached level {}!",
                player.level
            ),
            YELLOW,
        );

        let fighter = player.fighter.as_mut().unwrap();
        let mut choice = None;
        while choice.is_none() {
            choice = menu(
                "Level up! Choose a stat to taise:\n",
                &[
                    format!("Constitution (+20 HP, from {})", fighter.max_hp),
                    format!("Strength (+1 attack, from {})", fighter.power),
                    format!("Agility (+1 defense, from {})", fighter.defense),
                ],
                LEVEL_SCREEN_WIDTH,
                &mut tcod.root,
            )
        }
        fighter.xp -= level_up_xp;
        match choice.unwrap() {
            0 => {
                fighter.max_hp += 20;
                fighter.hp += 20;
            }
            1 => {
                fighter.power += 1;
            }
            2 => {
                fighter.defense += 1;
            }
            _ => unreachable!(),
        }
    }
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

fn make_map(objects: &mut Vec<Object>) -> Map {
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
        fill_with_objects(&room, &map, objects);
    }

    // stairs to go one level deeper
    let last_room = rooms[rooms.len() - 1].center();
    let mut stairs = Object::new(last_room.0, last_room.1, '<', WHITE, "stairs", false);
    stairs.always_visible = true;
    objects.push(stairs);

    map
}

fn is_blocked(x: i32, y: i32, map: &Map, objects: &[Object]) -> bool {
    if map[x as usize][y as usize].blocked {
        return true;
    }

    objects
        .iter()
        .any(|object| object.pos() == (x, y) && object.blocks)
}

fn is_out_of_bounds(x: i32, y: i32) -> bool {
    x < 0 || x >= MAP_WIDTH || y < 0 || y >= MAP_HEIGHT
}

fn create_orc(x: i32, y: i32) -> Object {
    let mut orc = Object::new(x, y, 'o', DESATURATED_GREEN, "orc", true);
    orc.alive = true;
    orc.fighter = Some(Fighter {
        max_hp: 10,
        hp: 10,
        defense: 0,
        power: 3,
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
        max_hp: 16,
        hp: 16,
        defense: 1,
        power: 4,
        xp: 100,
        on_death: DeathCallback::Monster,
    });
    troll.ai = Some(Ai::Basic);
    troll
}

fn fill_with_objects(room: &Rectangle, map: &Map, objects: &mut Vec<Object>) {
    let monster_count = rand::thread_rng().gen_range(0, MAX_ROOM_MONSTERS + 1);

    for _ in 0..monster_count {
        let x = rand::thread_rng().gen_range(room.x1 + 1, room.x2);
        let y = rand::thread_rng().gen_range(room.y1 + 1, room.y2);

        if is_blocked(x, y, map, objects) {
            continue;
        }

        let monster = if rand::random::<f32>() < 0.8 {
            // 80% chance of getting an orc
            create_orc(x, y)
        } else {
            create_troll(x, y)
        };

        objects.push(monster);
    }

    let num_items = rand::thread_rng().gen_range(0, MAX_ROOM_ITEMS + 1);

    for _ in 0..num_items {
        let x = rand::thread_rng().gen_range(room.x1 + 1, room.x2);
        let y = rand::thread_rng().gen_range(room.y1 + 1, room.y2);

        if !is_blocked(x, y, map, objects) {
            let dice = rand::random::<f32>();
            let mut item = if dice < 0.7 {
                // create a healing potion (70% chance)
                let mut object = Object::new(x, y, '!', VIOLET, "healing potion", false);
                object.item = Some(Item::Heal);
                object
            } else if dice < 0.7 + 0.1 {
                // create a lightning bolt scroll (10% chance)
                let mut object =
                    Object::new(x, y, '#', LIGHT_YELLOW, "scroll of lightning bolt", false);
                object.item = Some(Item::Lightning);
                object
            } else if dice < 0.7 + 0.1 + 0.1 {
                // create a fireball scroll (10% chance)
                let mut object = Object::new(x, y, '#', LIGHT_YELLOW, "scroll of fireball", false);
                object.item = Some(Item::Fireball);
                object
            } else {
                // create a confusion scroll (10% chance)
                let mut object = Object::new(x, y, '#', LIGHT_YELLOW, "scroll of confusion", false);
                object.item = Some(Item::Confusion);
                object
            };
            item.always_visible = true;
            objects.push(item);
        }
    }
}

fn menu<T: AsRef<str>>(header: &str, options: &[T], width: i32, root: &mut Root) -> Option<usize> {
    assert!(
        options.len() <= 26,
        "Can't have a menu with more than 26 options."
    );
    let header_height = if header.is_empty() {
        0
    } else {
        root.get_height_rect(0, 0, width, SCREEN_HEIGHT, header)
    };
    let height = options.len() as i32 + header_height;

    let mut window = Offscreen::new(width, height);
    window.set_default_foreground(WHITE);
    window.print_rect_ex(
        0,
        0,
        width,
        height,
        BackgroundFlag::None,
        TextAlignment::Left,
        header,
    );

    for (index, text) in options.iter().enumerate() {
        let letter = (b'a' + index as u8) as char;
        let text = format!("({}) {}", letter, text.as_ref());
        window.print_ex(
            0,
            header_height + index as i32,
            BackgroundFlag::None,
            TextAlignment::Left,
            text,
        );
    }

    let x = (SCREEN_WIDTH - width) / 2;
    let y = (SCREEN_HEIGHT - height) / 2;
    blit(&window, (0, 0), (width, height), root, (x, y), 1.0, 0.7);

    root.flush();
    let key = root.wait_for_keypress(true);

    if key.printable.is_alphabetic() {
        let index = key.printable.to_ascii_lowercase() as usize - 'a' as usize;
        if index < options.len() {
            Some(index)
        } else {
            None
        }
    } else {
        None
    }
}

fn msgbox(text: &str, width: i32, root: &mut Root) {
    let options: Vec<&str> = vec![];
    menu(text, &options, width, root);
}

fn inventory_menu(inventory: &[Object], header: &str, root: &mut Root) -> Option<usize> {
    let options = if inventory.len() == 0 {
        vec!["Inventory is empty.".into()]
    } else {
        inventory.iter().map(|i| i.name.clone()).collect()
    };

    let index = menu(header, &options, INVENTORY_WIDTH, root);

    if inventory.len() > 0 {
        index
    } else {
        None
    }
}

fn vision_update(tcod: &mut Tcod, map: &mut Map, player: &Object) {
    // recompute fov
    tcod.fov
        .compute_fov(player.x, player.y, TORCH_RADIUS, FOV_LIGHT_WALLS, FOV_ALGO);

    // explore map
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            if tcod.fov.is_in_fov(x, y) {
                map[x as usize][y as usize].explored = true;
            }
        }
    }
}

fn render_bar(
    panel: &mut Offscreen,
    x: i32,
    y: i32,
    total_width: i32,
    name: &str,
    value: i32,
    maximum: i32,
    bar_color: Color,
    back_color: Color,
) {
    let bar_width = (value as f32 / maximum as f32 * total_width as f32) as i32;

    // render background
    panel.set_default_background(back_color);
    panel.rect(x, y, total_width, 1, false, BackgroundFlag::Screen);

    // render contents on top
    panel.set_default_background(bar_color);
    if bar_width > 0 {
        panel.rect(x, y, bar_width, 1, false, BackgroundFlag::Screen);
    }

    panel.set_default_foreground(WHITE);
    panel.print_ex(
        x + total_width / 2,
        y,
        BackgroundFlag::None,
        TextAlignment::Center,
        format!("{}: {}/{}", name, value, maximum),
    );
}

fn render_all(tcod: &mut Tcod, game: &Game, objects: &[Object]) {
    // render map
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            let tile = &game.map[x as usize][y as usize];
            let wall = tile.block_sight;
            let lit = tcod.fov.is_in_fov(x, y);
            let color = match (wall, lit) {
                (true, true) => COLOR_LIGHT_WALL,
                (true, false) => COLOR_DARK_WALL,
                (false, true) => COLOR_LIGHT_GROUND,
                (false, false) => COLOR_DARK_GROUND,
            };
            if tile.explored {
                tcod.con
                    .set_char_background(x, y, color, BackgroundFlag::Set);
            }
        }
    }

    let mut to_draw: Vec<_> = objects
        .iter()
        .filter(|o| o.always_visible || tcod.fov.is_in_fov(o.x, o.y))
        .collect();
    to_draw.sort_by(|lhs, rhs| lhs.blocks.cmp(&rhs.blocks));

    // render objects
    for obj in to_draw {
        obj.draw(&mut tcod.con);
    }

    // blit the contents of "con" to the root console and present it
    blit(
        &tcod.con,
        (0, 0),
        (MAP_WIDTH, MAP_HEIGHT),
        &mut tcod.root,
        (0, 0),
        1.0,
        1.0,
    );

    tcod.panel.set_default_background(BLACK);
    tcod.panel.clear();

    let (hp, max_hp) = if let Some(fighter) = objects[PLAYER].fighter {
        (fighter.hp, fighter.max_hp)
    } else {
        (0, 0)
    };

    render_bar(
        &mut tcod.panel,
        1,
        1,
        BAR_WIDTH,
        "HP",
        hp,
        max_hp,
        LIGHT_RED,
        DARKER_RED,
    );

    tcod.panel.print_ex(
        1,
        3,
        BackgroundFlag::None,
        TextAlignment::Left,
        format!("Dungeon level: {}", game.dungeon_level),
    );

    tcod.panel.set_default_foreground(LIGHT_GREY);
    tcod.panel.print_ex(
        1,
        0,
        BackgroundFlag::None,
        TextAlignment::Left,
        get_names_under_mouse(tcod.mouse, objects, &tcod.fov),
    );

    let mut y = MSG_HEIGHT as i32;
    for &(ref msg, color) in game.messages.iter().rev() {
        let msg_height = tcod.panel.get_height_rect(MSG_X, y, MSG_WIDTH, 0, msg);
        y -= msg_height;
        if y < 0 {
            break;
        }
        tcod.panel.set_default_foreground(color);
        tcod.panel.print_rect(MSG_X, y, MSG_WIDTH, 0, msg);
    }

    blit(
        &tcod.panel,
        (0, 0),
        (SCREEN_WIDTH, SCREEN_HEIGHT),
        &mut tcod.root,
        (0, PANEL_Y),
        1.0,
        1.0,
    );
}

fn get_names_under_mouse(mouse: Mouse, objects: &[Object], fov_map: &FovMap) -> String {
    let (x, y) = (mouse.cx as i32, mouse.cy as i32);

    let names = objects
        .iter()
        .filter(|o| o.pos() == (x, y) && fov_map.is_in_fov(o.x, o.y))
        .map(|o| o.name.clone())
        .collect::<Vec<_>>();

    names.join(", ")
}

struct Tcod {
    root: Root,
    con: Offscreen,
    panel: Offscreen,
    fov: FovMap,
    key: Key,
    mouse: Mouse,
}

fn handle_keys(tcod: &mut Tcod, game: &mut Game, objects: &mut Vec<Object>) -> PlayerAction {
    use tcod::input::Key;
    use tcod::input::KeyCode::*;
    use PlayerAction::*;

    match (tcod.key, tcod.key.text(), objects[PLAYER].alive) {
        (
            Key {
                code: Enter,
                alt: true,
                ..
            },
            _,
            _,
        ) => {
            // Alt+Enter: toggle fullscreen
            let fullscreen = tcod.root.is_fullscreen();
            tcod.root.set_fullscreen(!fullscreen);
            return DidntTakeTurn;
        }
        (Key { code: Escape, .. }, _, _) => return Exit, // exit game

        // pick up an item
        (Key { code: Text, .. }, "g", true) => {
            let item_id = objects
                .iter()
                .position(|o| o.pos() == objects[PLAYER].pos() && o.item.is_some());
            if let Some(id) = item_id {
                pick_item_up(id, game, objects)
            }
            DidntTakeTurn
        }

        // drop an item
        (Key { code: Text, .. }, "d", true) => {
            if let Some(choice) = inventory_menu(
                &game.inventory,
                "Press the key next to an item to drop it, or any other to cancel\n",
                &mut tcod.root,
            ) {
                drop_item(choice, game, objects);
            }

            DidntTakeTurn
        }

        // open inventory and optionally use the item
        (Key { code: Text, .. }, "i", true) => {
            if let Some(choice) = inventory_menu(
                &game.inventory,
                "Press the key next to an item to use it, or any other to cancel\n",
                &mut tcod.root,
            ) {
                use_item(choice, tcod, game, objects)
            }

            DidntTakeTurn
        }

        // show character information
        (Key { code: Text, .. }, "c", true) => {
            let player = &objects[PLAYER];
            let level = player.level;
            let level_up_xp = player.level_up_xp();
            if let Some(fighter) = player.fighter.as_ref() {
                let msg = format!(
                    "Character information

Level: {}
Experience: {}
Experience to level up: {}

Maximum HP: {}
Attack: {}
Defence: {}",
                    level, fighter.xp, level_up_xp, fighter.max_hp, fighter.power, fighter.defense
                );
                msgbox(&msg, CHARACTER_SCREEN_WIDTH, &mut tcod.root);
            }

            DidntTakeTurn
        }

        // go down stairs if the player is on them
        (Key { code: Text, .. }, "<", true) => {
            let player_on_stairs = objects
                .iter()
                .any(|o| o.pos() == objects[PLAYER].pos() && o.name == "stairs");
            if player_on_stairs {
                next_level(tcod, game, objects);
            }

            DidntTakeTurn
        }

        // do nothing i. e. wait for the monster to come to you
        (Key { code: Spacebar, .. }, _, true) | (Key { code: NumPad5, .. }, _, true) => TookTurn,

        // movement keys
        (Key { code: Up, .. }, _, true) | (Key { code: NumPad8, .. }, _, true) => {
            player_move_or_attack(0, -1, game, objects);
            TookTurn
        }
        (Key { code: Down, .. }, _, true) | (Key { code: NumPad2, .. }, _, true) => {
            player_move_or_attack(0, 1, game, objects);
            TookTurn
        }
        (Key { code: Left, .. }, _, true) | (Key { code: NumPad4, .. }, _, true) => {
            player_move_or_attack(-1, 0, game, objects);
            TookTurn
        }
        (Key { code: Right, .. }, _, true) | (Key { code: NumPad6, .. }, _, true) => {
            player_move_or_attack(1, 0, game, objects);
            TookTurn
        }
        // diagonals
        (Key { code: NumPad7, .. }, _, true) => {
            player_move_or_attack(-1, -1, game, objects);
            TookTurn
        }
        (Key { code: NumPad9, .. }, _, true) => {
            player_move_or_attack(1, -1, game, objects);
            TookTurn
        }
        (Key { code: NumPad1, .. }, _, true) => {
            player_move_or_attack(-1, 1, game, objects);
            TookTurn
        }
        (Key { code: NumPad3, .. }, _, true) => {
            player_move_or_attack(1, 1, game, objects);
            TookTurn
        }

        // etc
        _ => return DidntTakeTurn,
    }
}

fn initialize_fov(tcod: &mut Tcod, map: &Map) {
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            let tile = &map[x as usize][y as usize];
            tcod.fov.set(x, y, !tile.block_sight, !tile.blocked);
        }
    }

    tcod.con.clear();
}

fn save_game(game: &Game, objects: &[Object]) -> Result<(), Box<dyn Error>> {
    let save_data = serde_json::to_string(&(game, objects))?;
    let mut file = File::create(SAVEGAME_FILE)?;
    file.write_all(save_data.as_bytes())?;
    Ok(())
}
fn load_game() -> Result<(Game, Vec<Object>), Box<dyn Error>> {
    let mut json_save_state = String::new();
    let mut file = File::open(SAVEGAME_FILE)?;
    file.read_to_string(&mut json_save_state)?;
    let result = serde_json::from_str::<(Game, Vec<Object>)>(&json_save_state)?;
    Ok(result)
}

fn new_game(tcod: &mut Tcod) -> (Game, Vec<Object>) {
    // game objects
    let mut player = Object::new(0, 0, '@', WHITE, "player", true);
    player.alive = true;
    player.fighter = Some(Fighter {
        max_hp: 30,
        hp: 30,
        defense: 2,
        power: 5,
        xp: 0,
        on_death: DeathCallback::Player,
    });

    let mut objects = vec![player];

    // game map + message log
    let mut game = Game {
        map: make_map(&mut objects),
        messages: Messages::new(),
        inventory: vec![],
        dungeon_level: 1,
    };

    initialize_fov(tcod, &game.map);

    game.messages.add(
        "Welcome stranger! Prepare to perish in the Tombs of the Ancient Kings.",
        RED,
    );

    (game, objects)
}

fn play_game(tcod: &mut Tcod, game: &mut Game, objects: &mut Vec<Object>) {
    // game loop
    let mut previous_player_position = (-1, -1);
    while !tcod.root.window_closed() {
        if objects[PLAYER].pos() != previous_player_position {
            vision_update(tcod, &mut game.map, &objects[PLAYER]);
        }

        match input::check_for_event(input::MOUSE | input::KEY_PRESS) {
            Some((_, Event::Mouse(m))) => tcod.mouse = m,
            Some((_, Event::Key(k))) => tcod.key = k,
            _ => tcod.key = Default::default(),
        }

        tcod.con.clear();
        render_all(tcod, game, &objects);
        tcod.root.flush();

        level_up(tcod, game, objects);

        previous_player_position = objects[PLAYER].pos();
        let action = handle_keys(tcod, game, objects);
        if action == PlayerAction::Exit {
            save_game(game, objects).unwrap();
            break;
        }
        if action != PlayerAction::DidntTakeTurn && objects[PLAYER].alive {
            // only if object is not player
            for id in 1..objects.len() {
                if objects[id].ai.is_some() {
                    ai_take_turn(id, tcod, game, objects)
                }
            }
        }
    }
}

fn next_level(tcod: &mut Tcod, game: &mut Game, objects: &mut Vec<Object>) {
    game.messages.add(
        "You take a moment to rest, and recover your strength.",
        VIOLET,
    );
    let heal_hp = objects[PLAYER].fighter.map_or(0, |f| f.max_hp / 2);
    objects[PLAYER].heal(heal_hp);

    game.messages.add(
        "After a rare moment of peace, you descend deeper into \
        the heart of the dungeon...",
        RED,
    );
    game.dungeon_level += 1;
    game.map = make_map(objects);
    initialize_fov(tcod, &game.map);
}

fn main_menu(tcod: &mut Tcod) {
    let img = tcod::image::Image::from_file("menu_background.png")
        .ok()
        .expect("Background image not found");

    while !tcod.root.window_closed() {
        // show the background image, at twice the regular console resolution
        tcod::image::blit_2x(&img, (0, 0), (-1, -1), &mut tcod.root, (0, 0));

        tcod.root.set_default_foreground(LIGHT_YELLOW);
        tcod.root.print_ex(
            SCREEN_WIDTH / 2,
            SCREEN_HEIGHT / 2 - 4,
            BackgroundFlag::None,
            TextAlignment::Center,
            "CHASM OF THE UNDERWORLD",
        );
        tcod.root.print_ex(
            SCREEN_WIDTH / 2,
            SCREEN_HEIGHT - 2,
            BackgroundFlag::None,
            TextAlignment::Center,
            "made by babysitterd",
        );

        let choices = &["Play a new game", "Continue last game", "Quit"];
        let choice = menu("", choices, 24, &mut tcod.root);

        match choice {
            Some(0) => {
                // new game
                let (mut game, mut objects) = new_game(tcod);
                play_game(tcod, &mut game, &mut objects);
            }
            Some(1) => {
                // load game
                match load_game() {
                    Ok((mut game, mut objects)) => {
                        initialize_fov(tcod, &game.map);
                        play_game(tcod, &mut game, &mut objects);
                    }
                    Err(_) => {
                        msgbox("\nNo saved game to load.\n", 24, &mut tcod.root);
                        continue;
                    }
                }
            }
            Some(2) => {
                // quit
                break;
            }
            _ => {}
        }
    }
}

fn main() {
    tcod::system::set_fps(LIMIT_FPS);

    let root = Root::initializer()
        .font("arial10x10.png", FontLayout::Tcod)
        .font_type(FontType::Greyscale)
        .size(SCREEN_WIDTH, SCREEN_HEIGHT)
        .title("Rust/libtcod tutorial")
        .init();

    let mut tcod = Tcod {
        root,
        con: Offscreen::new(MAP_WIDTH, MAP_HEIGHT),
        panel: Offscreen::new(SCREEN_WIDTH, PANEL_HEIGHT),
        fov: FovMap::new(MAP_WIDTH, MAP_HEIGHT),
        key: Default::default(),
        mouse: Default::default(),
    };

    // let (mut game, mut objects) = new_game(&mut tcod);
    // play_game(&mut tcod, &mut game, &mut objects);
    main_menu(&mut tcod);
}
