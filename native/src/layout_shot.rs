//! Renders the main window offscreen to PNG files, to check the layout at a
//! given window size without a screen. Run by hand:
//! `OXIGRAM_SHOT_DIR=some/folder cargo test -p oxigram-native layout_shot -- --ignored`

use std::rc::Rc;

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType, Rgb565Pixel};
use slint::platform::{Platform, PlatformError, WindowAdapter};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::{AppWindow, DialogItem, ProfileLine};

struct Offscreen(Rc<MinimalSoftwareWindow>);

impl Platform for Offscreen {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.0.clone())
    }
}

fn save(window: &MinimalSoftwareWindow, width: u32, height: u32, path: &std::path::Path) {
    let mut buffer = vec![Rgb565Pixel(0); (width * height) as usize];
    window.request_redraw();
    window.draw_if_needed(|renderer| {
        renderer.render(&mut buffer, width as usize);
    });
    let mut image = image::RgbImage::new(width, height);
    for (pixel, px) in image.pixels_mut().zip(&buffer) {
        let v = px.0;
        let r = ((v >> 11) & 0x1f) as u8;
        let g = ((v >> 5) & 0x3f) as u8;
        let b = (v & 0x1f) as u8;
        *pixel = image::Rgb([r << 3 | r >> 2, g << 2 | g >> 4, b << 3 | b >> 2]);
    }
    image.save(path).unwrap();
}

#[test]
#[ignore]
fn layout_shot() {
    let Ok(dir) = std::env::var("OXIGRAM_SHOT_DIR") else { return };
    let dir = std::path::PathBuf::from(dir);
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen(window.clone()))).unwrap();
    let ui = AppWindow::new().unwrap();
    ui.set_stage("ready".into());
    let dialogs: Vec<DialogItem> = (0..14)
        .map(|i| DialogItem {
            title: format!("Чат номер {i} с длинным названием").into(),
            initial: "Ч".into(),
            preview: "Последнее сообщение в этом чате, довольно длинное, чтобы обрезаться".into(),
            time: "12:34".into(),
            unread: if i % 3 == 0 { 5 } else { 0 },
            kind: "channel".into(),
            active: i == 2,
            ..Default::default()
        })
        .collect();
    ui.set_dialogs(ModelRc::new(VecModel::from(dialogs)));
    ui.set_chat_open(true);
    ui.set_chat_title("Канал с очень длинным названием, которое не помещается в шапку".into());
    let lines = vec![
        ProfileLine {
            value: "Описание канала: https://example.com/a/very/long/link/without/any/spaces/that/goes/on/and/on/and/on/forever".into(),
            label: "Описание".into(),
        },
        ProfileLine { value: "@channel_name".into(), label: "Ссылка".into() },
    ];
    ui.set_info_lines(ModelRc::new(VecModel::from(lines)));
    let messages: Vec<crate::MessageItem> = (0..4)
        .map(|i| crate::MessageItem {
            id: i,
            text: "Пост канала с довольно длинным текстом, чтобы он переносился на несколько строк и занимал ширину колонки целиком. https://example.com/very/long/link/without/spaces/at/all/that/goes/on/and/on".into(),
            time: "12:00".into(),
            wide: i % 2 == 0,
            outgoing: i == 3,
            multiline: true,
            tail: true,
            media: if i == 5 { "Фото".into() } else { "".into() },
            media_w: if i == 5 { 600.0 } else { 0.0 },
            media_h: if i == 5 { 400.0 } else { 0.0 },
            files: if i >= 2 {
                ModelRc::new(VecModel::from(
                    (0..3)
                        .map(|state| crate::FileItem {
                            id: i,
                            name: format!("Отчёт за квартал номер {state} с длинным именем.pdf").into(),
                            ext: "PDF".into(),
                            size: if state == 1 { "0,4 МБ из 1,2 МБ".into() } else { "1,2 МБ".into() },
                            state,
                            progress: 0.33,
                        })
                        .collect::<Vec<_>>(),
                ))
            } else {
                Default::default()
            },
            ..Default::default()
        })
        .collect();
    ui.set_messages(ModelRc::new(VecModel::from(messages)));
    ui.set_info_title("Канал".into());
    for (width, height) in [(1690u32, 900u32), (1352, 800), (1170, 800), (1100, 800), (900, 700)] {
        window.set_size(slint::PhysicalSize::new(width, height));
        ui.show().unwrap();
        for open in [false, true] {
            ui.set_info_open(open);
            save(&window, width, height, &dir.join(format!("shot-{width}-{}.png", if open { "info" } else { "plain" })));
        }
    }
}
