pub mod map;

use crate::config::*;
use crate::game::map::{is_blocked, is_out_of_bounds, make_map, Map, MAP_HEIGHT, MAP_WIDTH};
use crate::object::ai::ai_take_turn;
use crate::object::equipment::{Equipment, Slot};
use crate::object::fighter::{DeathCallback, Fighter};
use crate::object::item::Item;
use crate::object::Object;

use tcod::colors::*;
use tcod::console::*;

use tcod::input::{self, Event, Key, Mouse};
use tcod::map::FovAlgorithm;
use tcod::map::Map as FovMap;

use std::error::Error;
use std::fs::File;
use std::io::{Read, Write};

use serde::{Deserialize, Serialize};

pub struct Tcod {
    pub root: Root,
    pub con: Offscreen,
    pub panel: Offscreen,
    pub fov: FovMap,
    pub key: Key,
    pub mouse: Mouse,
    pub ignore_next_event: bool,
}

impl Tcod {
    pub fn new(root: Root) -> Self {
        Tcod {
            root,
            con: Offscreen::new(MAP_WIDTH, MAP_HEIGHT),
            panel: Offscreen::new(SCREEN_WIDTH, PANEL_HEIGHT),
            fov: FovMap::new(MAP_WIDTH, MAP_HEIGHT),
            key: Default::default(),
            mouse: Default::default(),
            ignore_next_event: false,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Game {
    pub map: Map,
    pub messages: Messages,
    pub inventory: Vec<Object>,
    pub dungeon_level: u32,
}

#[derive(Serialize, Deserialize)]
pub struct Messages {
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

const SAVEGAME_FILE: &str = "savegame.dat";

const FOV_ALGO: FovAlgorithm = FovAlgorithm::Basic; // default FOV algorithm
const FOV_LIGHT_WALLS: bool = true; // light walls or not
const TORCH_RADIUS: i32 = 10;

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

/// move by the given amount, if the destination is not blocked
pub fn move_by(id: usize, dx: i32, dy: i32, game: &mut Game, objects: &mut [Object]) {
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

#[derive(Clone, Copy, Debug, PartialEq)]
enum PlayerAction {
    TookTurn,
    DidntTakeTurn,
    Exit,
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
        let slot = item.equipment.map(|e| e.slot);
        game.inventory.push(item);

        if let Some(slot) = slot {
            if Equipment::get_equipped_in_slot(slot, &game.inventory).is_none() {
                let index = game.inventory.len() - 1;
                game.inventory[index].equip(&mut game.messages);
            }
        }
    }
}

fn drop_item(id: usize, game: &mut Game, objects: &mut Vec<Object>) {
    let mut item = game.inventory.remove(id);
    if item.equipment.is_some() {
        item.dequip(&mut game.messages);
    }
    item.set_pos(objects[PLAYER].x, objects[PLAYER].y);
    game.messages
        .add(format!("You dropped a {}.", item.name), YELLOW);
    objects.push(item);
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
                    format!("Constitution (+20 HP, from {})", fighter.base_max_hp),
                    format!("Strength (+1 attack, from {})", fighter.base_power),
                    format!("Agility (+1 defense, from {})", fighter.base_defense),
                ],
                LEVEL_SCREEN_WIDTH,
                tcod,
            )
        }
        fighter.xp -= level_up_xp;
        match choice.unwrap() {
            0 => {
                fighter.base_max_hp += 20;
                fighter.hp += 20;
            }
            1 => {
                fighter.base_power += 1;
            }
            2 => {
                fighter.base_defense += 1;
            }
            _ => unreachable!(),
        }
    }
}

fn menu<T: AsRef<str>>(header: &str, options: &[T], width: i32, tcod: &mut Tcod) -> Option<usize> {
    assert!(
        options.len() <= 26,
        "Can't have a menu with more than 26 options."
    );
    let header_height = if header.is_empty() {
        0
    } else {
        tcod.root
            .get_height_rect(0, 0, width, SCREEN_HEIGHT, header)
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
    blit(
        &window,
        (0, 0),
        (width, height),
        &mut tcod.root,
        (x, y),
        1.0,
        0.7,
    );

    tcod.root.flush();
    let key = tcod.root.wait_for_keypress(true);

    tcod.ignore_next_event = true;

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

fn msgbox(text: &str, width: i32, tcod: &mut Tcod) {
    let options: Vec<&str> = vec![];
    menu(text, &options, width, tcod);
}

fn inventory_menu(inventory: &[Object], header: &str, tcod: &mut Tcod) -> Option<usize> {
    let options = if inventory.len() == 0 {
        vec!["Inventory is empty.".into()]
    } else {
        inventory
            .iter()
            .map(|item| match item.equipment {
                Some(equipment) if equipment.equipped => {
                    format!("{} (on {})", item.name, equipment.slot)
                }
                _ => item.name.clone(),
            })
            .collect()
    };

    let index = menu(header, &options, INVENTORY_WIDTH, tcod);

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

pub fn render_all(tcod: &mut Tcod, game: &Game, objects: &[Object]) {
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

    let hp = objects[PLAYER].fighter.map_or(0, |f| f.hp);
    let max_hp = objects[PLAYER].max_hp(game);

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
                tcod,
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
                tcod,
            ) {
                Item::use_item(choice, tcod, game, objects)
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
Defense: {}",
                    level,
                    fighter.xp,
                    level_up_xp,
                    player.max_hp(game),
                    player.power(game),
                    player.defense(game),
                );
                msgbox(&msg, CHARACTER_SCREEN_WIDTH, tcod);
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
        base_max_hp: 100,
        hp: 100,
        base_defense: 1,
        base_power: 2,
        xp: 0,
        on_death: DeathCallback::Player,
    });

    let mut objects = vec![player];

    // game map + message log
    let mut game = Game {
        map: make_map(&mut objects, 1),
        messages: Messages::new(),
        inventory: vec![],
        dungeon_level: 1,
    };

    let mut dagger = Object::new(0, 0, '-', SKY, "dagger", false);
    dagger.item = Some(Item::Sword);
    dagger.equipment = Some(Equipment {
        equipped: true,
        slot: Slot::RightHand,
        max_hp_bonus: 0,
        defense_bonus: 0,
        power_bonus: 2,
    });
    game.inventory.push(dagger);

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

        if tcod.ignore_next_event {
            tcod.ignore_next_event = false;
            continue;
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
    let heal_hp = objects[PLAYER].max_hp(game) / 2;
    objects[PLAYER].heal(heal_hp, game);

    game.messages.add(
        "After a rare moment of peace, you descend deeper into \
        the heart of the dungeon...",
        RED,
    );
    game.dungeon_level += 1;
    game.map = make_map(objects, game.dungeon_level);
    initialize_fov(tcod, &game.map);
}

pub fn main_menu(tcod: &mut Tcod) {
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
        let choice = menu("", choices, 24, tcod);

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
                        msgbox("\nNo saved game to load.\n", 24, tcod);
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
