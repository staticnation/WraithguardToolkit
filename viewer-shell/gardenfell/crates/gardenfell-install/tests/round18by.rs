/*! Round 18by — which copy of a file an MO2 install serves, and in what order.
 *
 * Robin, reading the shell's overlay listing: *"the #1 in the list is overwrite, #2 is
 * Distant Land etc., which means we are presenting them in the opposite order compared to
 * MO2 does. How we show them is less important, but in MO2, the next to last mod read is
 * Distant Land, and the very last is overwrite, and later always win and overwrite files
 * from earlier. There might be a difference there compared to the page app?"*
 *
 * There is not — both front ends call `layout::mo2_profile` and neither reorders what it
 * gets back. But nothing said so, and this program has found the *same class* of shell
 * versus window disagreement three times in eight rounds: no archives (18bo), no profile
 * (18bh), no surfaces (18bu). "I checked and they agree" is worth exactly as much as the
 * test that keeps them agreeing, so here is the test.
 *
 * **The two conventions, and why they look opposed.** MO2 stores `modlist.txt` with the
 * *highest* priority mod on the first line, and displays the same list upside down, so the
 * winner is at the bottom of the window and `overwrite` is below everything. Gardenfell
 * keeps one order everywhere — **the winner first** — because that is the order a lookup
 * wants: `Vfs::resolve` walks the roots and takes the first root that has the file. So the
 * stack is `overwrite`, then the mods in `modlist.txt` order, then the game's own
 * `Data Files` at the bottom; read against MO2's window that list is upside down, and it
 * describes exactly the same precedence.
 *
 * What is pinned below is not the *display* but the answer: the same file put in four
 * places, and which one comes out.
 *
 * **Round 18cb flipped the display, and only the display.** Robin, reading the listing a
 * second time: *"I still don't like that the MO2 reading feels inverted. #1 Overwrite should
 * be the last entry that overrides anything that was loaded before ... #918 Data Files should
 * be the first that is then overwritten."* Both front ends print the stack base-first now, so
 * the last line is the winner, which is the direction MO2's own window reads in. The engine's
 * order did not move and must not: `Vfs::resolve` takes the first root that has the file, so
 * reversing `layout`'s output to make a diagnostic read nicely would invert the precedence of
 * every file in the install. `the_overlay_is_ordered_winner_first_and_the_game_is_at_the_bottom`
 * is the guard on that, and `the_listing_is_the_same_stack_the_other_way_up` ties the printed
 * direction to the answer, so the two cannot drift apart into a listing that lies.
 */

use gardenfell_install::layout;
use gardenfell_install::vfs::{Loc, Vfs};
use std::path::{Path, PathBuf};

struct Dir(PathBuf);
impl Dir {
    fn new(tag: &str) -> Dir {
        let mut p = std::env::temp_dir();
        p.push(format!("gf_18by_{}_{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Dir(p)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const INI: &str = "[Game Files]\nGameFile0=Morrowind.esm\n";
/// The file every layer of the install ships a different copy of.
const WANTED: &str = "textures/contested.dds";

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

/// An MO2 instance with `overwrite`, two mods and the game's own `Data Files`, each
/// shipping its own copy of one texture.
///
/// `modlist.txt` is written the way MO2 writes it — highest priority first — and `Top` is
/// therefore the mod that beats `Bottom`. Getting that the wrong way round in this fixture
/// would make every assertion below pass for the wrong reason, so it is stated here once:
/// **first line wins.**
fn install(base: &Path, game: &Path, layers: &[(&str, &str)]) {
    std::fs::create_dir_all(base.join("mods/Top")).unwrap();
    std::fs::create_dir_all(base.join("mods/Bottom")).unwrap();
    std::fs::create_dir_all(base.join("mods/Disabled")).unwrap();
    std::fs::create_dir_all(base.join("overwrite")).unwrap();
    std::fs::create_dir_all(game.join("Data Files")).unwrap();
    std::fs::write(
        base.join("ModOrganizer.ini"),
        format!(
            "[General]\nselected_profile=@ByteArray(Default)\ngamePath=@ByteArray({})\n",
            game.display()
        ),
    )
    .unwrap();
    std::fs::create_dir_all(base.join("profiles/Default")).unwrap();
    // Top of the file is the top of the priority stack, and `-` is a mod switched off.
    std::fs::write(base.join("profiles/Default/modlist.txt"), "+Top\n-Disabled\n+Bottom\n").unwrap();
    std::fs::write(base.join("profiles/Default/Morrowind.ini"), INI).unwrap();
    std::fs::write(game.join("Data Files/Morrowind.esm"), "x").unwrap();
    // A copy in the disabled mod too: if it is ever served, the profile means nothing.
    write(&base.join("mods/Disabled").join(WANTED), "disabled");

    for (where_, body) in layers {
        let at = match *where_ {
            "overwrite" => base.join("overwrite"),
            "top" => base.join("mods/Top"),
            "bottom" => base.join("mods/Bottom"),
            "vanilla" => game.join("Data Files"),
            other => panic!("no such layer {other}"),
        };
        write(&at.join(WANTED), body);
    }
}

/// The overlay both front ends build: `layout` decides the order, `Vfs::with_roots` indexes
/// them in it, and nothing in between is allowed to sort.
fn overlay(base: &Path) -> (Vec<String>, Vfs) {
    let l = layout::mo2_profile(base, Some("Default")).expect("an MO2 install");
    let dirs: Vec<(String, PathBuf)> = l
        .roots
        .iter()
        .map(|p| (p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), p.clone()))
        .collect();
    let names = dirs.iter().map(|(l, _)| l.clone()).collect();
    (names, Vfs::with_roots(&dirs))
}

/// What the overlay actually hands back for the contested file.
fn served(v: &Vfs) -> String {
    match v.resolve(WANTED) {
        Some(Loc::Disk(p)) => std::fs::read_to_string(p).unwrap(),
        Some(Loc::Packed { .. }) => "packed".into(),
        None => "nothing".into(),
    }
}

/// The stack, in the order a lookup walks it: overwrite, the mods by descending priority,
/// the game last. A mod switched off is not in it at all.
#[test]
fn the_overlay_is_ordered_winner_first_and_the_game_is_at_the_bottom() {
    let d = Dir::new("order");
    let g = Dir::new("order_game");
    install(d.path(), g.path(), &[("overwrite", "overwrite")]);
    let (names, _) = overlay(d.path());
    assert_eq!(
        names,
        vec!["overwrite", "Top", "Bottom", "Data Files"],
        "the overlay order is the priority order, highest first"
    );
}

/// **The answer Robin asked for.** Four copies of one texture, and the one that comes out
/// is `overwrite`'s — which is what MO2 shows at the very bottom of its list, and what the
/// game reads. Then each layer is taken away in turn and the next one down answers.
///
/// Done by removing rather than by four separate fixtures on purpose: it walks the whole
/// stack in one test, so a rule that held only at the top would fail here.
#[test]
fn every_layer_in_turn_wins_over_the_one_below_it() {
    let d = Dir::new("stack");
    let g = Dir::new("stack_game");
    install(
        d.path(),
        g.path(),
        &[("overwrite", "overwrite"), ("top", "top"), ("bottom", "bottom"), ("vanilla", "vanilla")],
    );

    let (_, v) = overlay(d.path());
    assert_eq!(served(&v), "overwrite", "MO2's overwrite folder beats every mod");

    std::fs::remove_file(d.path().join("overwrite").join(WANTED)).unwrap();
    let (_, v) = overlay(d.path());
    assert_eq!(served(&v), "top", "the higher-priority mod beats the lower one");

    std::fs::remove_file(d.path().join("mods/Top").join(WANTED)).unwrap();
    let (_, v) = overlay(d.path());
    assert_eq!(served(&v), "bottom", "and the lower mod beats the game's own copy");

    std::fs::remove_file(d.path().join("mods/Bottom").join(WANTED)).unwrap();
    let (_, v) = overlay(d.path());
    assert_eq!(served(&v), "vanilla", "with nothing above it, the game's own file is served");

    /* And the copy in the mod the profile has switched off is never served — not even when
       it is the only one left. A disabled mod is not in the overlay, which is the whole
       meaning of a profile. */
    std::fs::remove_file(g.path().join("Data Files").join(WANTED)).unwrap();
    let (_, v) = overlay(d.path());
    assert_eq!(served(&v), "nothing", "a switched-off mod is not a fallback");
}

/// The convention, stated as a test: `modlist.txt`'s first line is the winner.
///
/// This is the one thing in this file a future reader is most likely to "fix" backwards,
/// because MO2's own window shows the opposite order. Reversing the two lines has to change
/// which copy is served, or the order is not being read at all.
#[test]
fn the_first_line_of_modlist_is_the_highest_priority() {
    let d = Dir::new("flip");
    let g = Dir::new("flip_game");
    install(d.path(), g.path(), &[("top", "top"), ("bottom", "bottom")]);

    let (_, v) = overlay(d.path());
    assert_eq!(served(&v), "top");

    std::fs::write(d.path().join("profiles/Default/modlist.txt"), "+Bottom\n-Disabled\n+Top\n").unwrap();
    let (names, v) = overlay(d.path());
    assert_eq!(names, vec!["overwrite", "Bottom", "Top", "Data Files"]);
    assert_eq!(served(&v), "bottom", "swapping the lines swaps which mod wins");
}

/// The direction the front ends *print*, tied to the answer the overlay gives.
///
/// Round 18cb. A listing is only worth reading if its direction is a fact rather than a
/// convention someone remembers: this asserts that the stack shown base-first ends with the
/// root whose copy of the contested file actually comes out. Reverse `layout`'s order to
/// "fix" the display and this fails beside the winner-first test above, which is the pair
/// of failures that says what went wrong.
#[test]
fn the_listing_is_the_same_stack_the_other_way_up() {
    let d = Dir::new("shown");
    let g = Dir::new("shown_game");
    install(d.path(), g.path(), &[("overwrite", "overwrite")]);
    let (names, v) = overlay(d.path());

    // What both front ends print: the same list, the other way up.
    let shown: Vec<String> = names.iter().rev().cloned().collect();
    assert_eq!(
        shown,
        vec!["Data Files", "Bottom", "Top", "overwrite"],
        "the base is listed first and the winner last, as MO2's window shows it"
    );
    assert_eq!(
        served(&v),
        *shown.last().unwrap(),
        "the last root listed has to be the one whose file the game reads — otherwise the \
         listing is upside down and says so with confidence"
    );
    assert_ne!(
        served(&v),
        *shown.first().unwrap(),
        "and the first listed is the one everything overwrites"
    );
}
