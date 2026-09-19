//! The numbers a library card shows without loading the whole show.

use serde::{Deserialize, Serialize};

use crate::{BusKind, Direction, Platform, Show};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: String,
    pub name: String,
    pub platform: Platform,
    pub model: String,
    pub firmware: String,
    pub modified: String,
    pub inputs: usize,
    pub outputs: usize,
    pub channels: usize,
    pub named_channels: usize,
    pub auxes: usize,
    pub groups: usize,
    pub matrices: usize,
    pub fx: usize,
    pub dcas: usize,
    pub mute_groups: usize,
    pub scenes: usize,
    pub cues: usize,
    pub notes_dropped: usize,
}

impl Summary {
    pub fn of(show: &Show) -> Summary {
        Summary {
            id: show.id.clone(),
            name: show.meta.name.clone(),
            platform: show.platform,
            model: show.system.model.clone(),
            firmware: show.system.firmware.clone(),
            modified: show.meta.modified.clone(),
            inputs: show.sockets.iter().filter(|s| s.direction == Direction::In).count(),
            outputs: show.sockets.iter().filter(|s| s.direction == Direction::Out).count(),
            channels: show.channels.len(),
            named_channels: show.channels.iter().filter(|c| !c.label.trim().is_empty()).count(),
            auxes: show.buses_of(BusKind::Aux).count(),
            groups: show.buses_of(BusKind::Group).count(),
            matrices: show.buses_of(BusKind::Matrix).count(),
            fx: show.buses_of(BusKind::FxSend).count(),
            dcas: show.dcas.len(),
            mute_groups: show.mute_groups.len(),
            scenes: show.scenes.len(),
            cues: show.cues.len(),
            notes_dropped: show.notes.iter().filter(|n| n.level == crate::NoteLevel::Dropped).count(),
        }
    }
}
