// Entry script for level_01 (ADR-0030: a documented subset of Rhai).
//
// Attach this to an entity with `attach_script`, or from the Details panel's ScriptRef row.
// Hooks: on_start(), on_update(dt), on_collision(other), on_trigger(other).

fn on_start() {
    set_var("game.score", 0);
    log("level_01 started");
}

fn on_update(dt) {
    // Spin gently in place. `self_id()` is the entity this script is attached to.
    set_rot(self_id(), 0.0, rot_y(self_id()) + dt, 0.0);
}

fn on_trigger(other) {
    // Anything tagged `player` scores; everything else is ignored.
    if !has_tag(other, "player") {
        return;
    }
    set_var("game.score", get_var("game.score") + 1);
    hud_set("score_label", "Score " + get_var("game.score"));
}
