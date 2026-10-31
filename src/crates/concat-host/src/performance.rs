// SPDX-License-Identifier: AGPL-3.0-or-later

//! The editor's low-resource preview mode. Export never reads this state.

use std::sync::atomic::{AtomicBool, Ordering};

static LOW_RESOURCE: AtomicBool = AtomicBool::new(false);

/// Whether playback should request a shorter decode lookahead.
pub fn low_resource() -> bool {
    LOW_RESOURCE.load(Ordering::Relaxed)
}

/// Applies the selected resource limits to the live preview scheduler.
pub fn set_low_resource(low: bool) {
    if LOW_RESOURCE.swap(low, Ordering::Relaxed) == low {
        return;
    }
    let scheduler = crate::scheduler();
    let memory = crate::memory::total();
    let budget = if low {
        const MB: u64 = 1024 * 1024;
        let bytes = memory.map_or(128 * MB, |total| (total / 32).clamp(128 * MB, 256 * MB));
        usize::try_from(bytes).unwrap_or(128 * 1024 * 1024)
    } else {
        concat_media::ReaderPool::frame_budget(memory)
    };
    scheduler.pool().set_limits(budget, if low { 3 } else { 8 });
    scheduler.set_worker_limit(if low {
        2
    } else {
        concat_media::Prefetcher::default_workers()
    });
}
