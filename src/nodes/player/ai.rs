use macroquad::{
    experimental::{collections::storage, scene},
    math::vec2,
    rand,
    time::get_frame_time,
};

use crate::{
    capabilities::Weapon,
    nodes::{player::Input, Player},
    Resources,
};

pub struct Ai {
    jump_cooldown: f32,
    throw_cooldown: f32,
    keep_direction_until_event: bool,
    keep_direction_timeout: f32,
    fix_direction: i32,
}

impl Ai {
    pub fn new() -> Ai {
        Ai {
            jump_cooldown: 0.,
            keep_direction_until_event: false,
            keep_direction_timeout: 0.,
            fix_direction: 0,
            throw_cooldown: 0.,
        }
    }

    pub fn update(&mut self, player: &mut Player) -> Input {
        let foe = scene::find_nodes_by_type::<Player>().next().unwrap();

        let mut input = Input {
            right: self.fix_direction == 1,
            left: self.fix_direction == -1,
            ..Default::default()
        };

        let mut following_horiz = false;

        if (player.body.pos.x - foe.body.pos.x).abs() >= 50. {
            //
            if !self.keep_direction_until_event {
                following_horiz = true;
                if player.body.pos.x > foe.body.pos.x {
                    input.left = true;
                } else {
                    input.right = true;
                }
            }
        }

        if !self.keep_direction_until_event
            && (player.body.pos.y - foe.body.pos.y).abs() >= 50.
            && !following_horiz
        {
            self.fix_direction = if rand::gen_range(0, 2) == 0 { 1 } else { -1 };
            self.keep_direction_until_event = true;
        }

        let dir = if input.left {
            -1.
        } else if input.right {
            1.
        } else {
            0.
        };

        {
            let collision_world = &mut storage::get_mut::<Resources>().collision_world;

            let obstacle_soon = collision_world
                .collide_check(player.body.collider, player.body.pos + vec2(15. * dir, 0.));
            let cliff_soon = !collision_world
                .collide_check(player.body.collider, player.body.pos + vec2(5. * dir, 5.));
            let wants_descent = player.body.pos.y < foe.body.pos.y;

            if (cliff_soon || obstacle_soon) && self.keep_direction_timeout <= 0. {
                self.keep_direction_until_event = false;
                self.fix_direction = 0;
                self.keep_direction_timeout = 1.;
            }

            if (obstacle_soon || (!wants_descent && cliff_soon))
                && player.body.on_ground
                && self.jump_cooldown <= 0.
            {
                input.jump = true;
                self.jump_cooldown = 0.2;
            }
        }

        if rand::gen_range(0, 200) == 5 {
            self.fix_direction = if rand::gen_range(0, 2) == 0 { 1 } else { -1 };
            self.keep_direction_until_event = true;
        }

        if rand::gen_range(0, 800) == 5 {
            input.throw = true;
            self.throw_cooldown = 1.;
        }

        if player.body.pos.distance(foe.body.pos) <= 100. || rand::gen_range(0, 180) == 5 {
            //
            if player.state_machine.state() == Player::ST_NORMAL && player.weapon.is_some() {
                player.state_machine.set_state(Player::ST_SHOOT);
            }
        }

        if self.jump_cooldown >= 0. {
            self.jump_cooldown -= get_frame_time();
        }
        if self.throw_cooldown >= 0. {
            self.throw_cooldown -= get_frame_time();
        }

        if self.keep_direction_timeout >= 0. {
            self.keep_direction_timeout -= get_frame_time();
        }

        if self.throw_cooldown <= 0.0 {
            for weapon in scene::find_nodes_with::<Weapon>() {
                use crate::capabilities::WeaponTrait;

                let weapon_rect = weapon.collider();
                if weapon_rect.point().distance(player.body.pos) <= 80. {
                    input.throw = true;
                }
            }
            self.throw_cooldown = 1.;
        }

        input
    }
}
