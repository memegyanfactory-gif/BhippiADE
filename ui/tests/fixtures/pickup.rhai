// A collectible: bob in place, add to the score when the player touches it, and open the
// exit once every pickup in the level is gone.
fn on_start() {
    set_var("game.score", 0);
    hud_set("score_label", "Score 0");
}

fn on_update(dt) {
    let bob = sin(time() * 3.0) * 0.25;
    set_pos(self_id(), pos_x(self_id()), 1.0 + bob, pos_z(self_id()));
}

fn on_trigger(other) {
    if !has_tag(other, "player") {
        return;
    }
    let score = get_var("game.score") + 10;
    set_var("game.score", score);
    hud_set("score_label", "Score " + score);
    play_sound("assets/audio/pickup.ogg");
    destroy(self_id());
    if score >= 30 {
        load_level("level_02");
    }
}
