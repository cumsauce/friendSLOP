//! The Bevy side: fetching data files in the browser and reloading them on
//! desktop when they change.

use bevy::asset::{AssetLoader, LoadContext, io::Reader};
use bevy::prelude::*;

use super::DataFile;

/// The game's data files, loaded and kept fresh.
pub struct DataPlugin {
    pub files: Vec<&'static dyn DataFile>,
}

#[derive(Resource)]
struct Files {
    files: Vec<&'static dyn DataFile>,
    /// Browser: the fetches in flight.
    handles: Vec<(usize, Handle<RawData>)>,
    since_check: f32,
}

/// Fired when a data file changes on disk and was reloaded.
#[derive(Message, Clone, Debug)]
pub struct DataReloaded(pub &'static str);

impl Plugin for DataPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<RawData>()
            .register_asset_loader(RawLoader)
            .add_message::<DataReloaded>()
            .insert_resource(Files {
                files: self.files.clone(),
                handles: Vec::new(),
                since_check: 0.0,
            })
            .add_systems(Startup, fetch)
            .add_systems(Update, (take_fetched, watch));
    }
}

/// Every registered file is in.
pub fn all_loaded(files: &[&'static dyn DataFile]) -> bool {
    files.iter().all(|f| f.loaded())
}

/// Is every data file the game registered loaded?
pub fn ready(files: Option<Res<FilesReady>>) -> bool {
    files.is_some_and(|f| f.0)
}

/// Set once every registered data file is loaded (the loading curtain waits).
#[derive(Resource, Default)]
pub struct FilesReady(pub bool);

fn fetch(mut files: ResMut<Files>, assets: Res<AssetServer>, mut commands: Commands) {
    commands.init_resource::<FilesReady>();
    if cfg!(target_arch = "wasm32") {
        let handles = files
            .files
            .iter()
            .enumerate()
            .map(|(i, f)| (i, assets.load(f.path())))
            .collect();
        files.handles = handles;
    } else {
        // Desktop reads from disk now, so systems never see a default.
        for f in &files.files {
            f.refresh();
        }
    }
}

fn take_fetched(
    mut files: ResMut<Files>,
    raw: Res<Assets<RawData>>,
    mut ready: Option<ResMut<FilesReady>>,
) {
    let Files {
        files: list,
        handles,
        ..
    } = &mut *files;
    handles.retain(|(i, h)| {
        let Some(data) = raw.get(h) else {
            return true;
        };
        if let Some(f) = list.get(*i) {
            f.set_bytes(&data.0);
        }
        false
    });
    if let Some(ready) = ready.as_mut()
        && !ready.0
    {
        ready.0 = all_loaded(list);
    }
}

/// Desktop: re-reads changed files about once a second.
fn watch(time: Res<Time>, mut files: ResMut<Files>, mut out: MessageWriter<DataReloaded>) {
    files.since_check += time.delta_secs();
    if files.since_check < 1.0 || cfg!(target_arch = "wasm32") {
        return;
    }
    files.since_check = 0.0;
    for f in &files.files {
        if f.loaded() && f.refresh() {
            info!("reloaded {}", f.path());
            out.write(DataReloaded(f.path()));
        }
    }
}

/// A data file's raw bytes, as fetched by the asset server.
#[derive(Asset, TypePath, Debug)]
pub struct RawData(pub Vec<u8>);

#[derive(Default, TypePath)]
struct RawLoader;

impl AssetLoader for RawLoader {
    type Asset = RawData;
    type Settings = ();
    type Error = std::io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        _: &mut LoadContext<'_>,
    ) -> Result<RawData, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        Ok(RawData(bytes))
    }

    fn extensions(&self) -> &[&str] {
        &["ron", "json", "heights"]
    }
}
