use berm_gui::Workbench;
use bezel::{
    gpui::{
        self, App, AppContext as _, Bounds, Menu, MenuItem, TitlebarOptions, WindowBounds,
        WindowOptions, actions, point, px, size,
    },
    theme::{
        Theme,
        appearance::{self, AppearanceMode},
    },
    ui::{self, icons},
};

actions!(berm, [Quit]);

fn main() {
    gpui_platform::application()
        .with_assets(icons::Assets)
        .run(|cx: &mut App| {
            if let Err(error) = ui::register_fonts(cx) {
                eprintln!("FONT REGISTRATION FAILED: {error:?}");
            }
            appearance::init(AppearanceMode::System, cx);
            berm_gui::init(cx);
            // Without a menu item `cmd-q` does nothing: the standard items come
            // from a nib and there is no nib here.
            cx.on_action(|_: &Quit, cx: &mut App| cx.quit());
            cx.set_menus(vec![
                Menu::new("berm").items([MenuItem::action("Quit", Quit)]),
            ]);

            let bounds = Bounds::centered(None, size(px(1040.0), px(700.0)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_background: Theme::of(cx).window_background_appearance(),
                    // No titlebar: the rail runs to the top of the window and
                    // the lights sit in its gutter, aligned with its heading.
                    titlebar: Some(TitlebarOptions {
                        title: None,
                        appears_transparent: true,
                        traffic_light_position: Some(point(px(20.0), px(15.0))),
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    appearance::observe_window(window, cx).detach();
                    cx.new(Workbench::new)
                },
            )
            .unwrap();
            cx.activate(true);
        });
}
