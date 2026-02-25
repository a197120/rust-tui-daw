use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use std::collections::HashSet;

use crate::app::{App, AppMode};
use crate::drums::DrumKind;
use crate::synth::note_name;

// ── Top-level routing ─────────────────────────────────────────────────────────

/// Draw all panels simultaneously.  `app.mode` controls which panel has
/// keyboard focus (highlighted border), not what is visible.
pub fn draw(f: &mut Frame, app: &App, enhanced: bool) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // title bar      chunks[0]
            Constraint::Length(12), // piano keyboard  chunks[1]
            Constraint::Length(8),  // synth seq 1     chunks[2]
            Constraint::Length(8),  // synth seq 2     chunks[3]
            Constraint::Length(12), // drum machine    chunks[4]
            Constraint::Length(6),  // effects         chunks[5]
            Constraint::Length(4),  // status          chunks[6]
            Constraint::Length(6),  // scope           chunks[7]
            Constraint::Min(0),     // help            chunks[8]
        ])
        .split(area);

    draw_title(f, chunks[0], enhanced, app);
    draw_piano(f, chunks[1], app);
    draw_synth_seq(f, chunks[2], app);
    draw_synth_seq2(f, chunks[3], app);
    draw_drums(f, chunks[4], app);
    draw_effects(f, chunks[5], app);
    draw_status(f, chunks[6], app);
    draw_oscilloscope(f, chunks[7], app);
    draw_help(f, chunks[8], app);
}

// ── Title bar ─────────────────────────────────────────────────────────────────

fn draw_title(f: &mut Frame, area: Rect, enhanced: bool, app: &App) {
    let focus_label = match app.mode {
        AppMode::Play      => "Keyboard",
        AppMode::SynthSeq  => "Synth Seq",
        AppMode::SynthSeq2 => "Synth Seq 2",
        AppMode::Drums     => "Drums",
        AppMode::Effects   => "Effects",
    };
    let kb_mode  = if enhanced { "enhanced" } else { "fallback" };
    let seq_ind  = if app.seq_playing()  { "  ▶SEQ"  } else { "" };
    let seq2_ind = if app.seq2_playing() { "  ▶SEQ2" } else { "" };
    let drum_ind = if app.drum_playing() { "  ▶DRUM" } else { "" };
    let fx_ind   = app.fx_indicators();

    let text = format!(
        "  RustTuiSynth  ─  Focus: {}{}{}{}{}  ─  [{}]  ─  Tab/F2: cycle focus  F1: wave  F3: drums",
        focus_label, seq_ind, seq2_ind, drum_ind, fx_ind, kb_mode
    );
    let color = if enhanced { Color::Cyan } else { Color::Yellow };
    f.render_widget(
        Paragraph::new(text)
            .style(Style::default().fg(color).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL)),
        area,
    );
}

// ── Piano keyboard ────────────────────────────────────────────────────────────

fn draw_piano(f: &mut Frame, area: Rect, app: &App) {
    let focused = app.mode == AppMode::Play;
    let title = if focused {
        " ► Keyboard — [←→] Octave  [↑↓] Volume  [Z-M / Q-P] Play notes "
    } else {
        " Keyboard "
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(if focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        });
    let inner = block.inner(area);
    f.render_widget(block, area);
    render_piano_widget(f, inner, app.base_octave, &app.highlighted_notes());
}

fn render_piano_widget(f: &mut Frame, area: Rect, base_octave: i32, active: &HashSet<u8>) {
    let white_sem = [0u8, 2, 4, 5, 7, 9, 11];
    let has_black = [true, true, false, true, true, true, false];
    let black_sem = [1u8, 3, 0, 6, 8, 10, 0];
    let num_oct   = 2usize;
    let n_white   = white_sem.len() * num_oct + 1;
    let base_midi = (base_octave * 12 + 12) as u8;

    let lower_white = ["z","x","c","v","b","n","m"];
    let upper_white = ["q","w","e","r","t","y","u"];
    let lower_black = ["s","d"," ","g","h","j"," "];
    let upper_black = ["2","3"," ","5","6","7"," "];
    let note_names  = ["C","D","E","F","G","A","B"];

    let mut lines: Vec<Line> = Vec::new();

    // Top border
    {
        let mut s = vec![Span::raw("┌")];
        for i in 0..n_white { s.push(Span::raw("───")); if i < n_white-1 { s.push(Span::raw("┬")); } }
        s.push(Span::raw("┐"));
        lines.push(Line::from(s));
    }

    // Black key rows
    for row in 0..4usize {
        let mut s = vec![Span::raw("│")];
        for wi in 0..n_white {
            let oct = wi / 7; let local_wi = wi % 7;
            let hb = wi < n_white-1 && has_black[local_wi];

            let midi_w = if wi == n_white - 1 {
                base_midi + 24
            } else {
                base_midi + (oct as u8) * 12 + white_sem[local_wi]
            };
            let w_active = active.contains(&midi_w);

            let left_black = if local_wi > 0 { has_black[local_wi-1] } else { oct > 0 && has_black[6] };
            let midi_lb = if local_wi > 0 && has_black[local_wi-1] {
                base_midi + (oct as u8) * 12 + black_sem[local_wi - 1]
            } else { 0 };
            let lb_active  = left_black && active.contains(&midi_lb);
            let midi_rb = if hb { base_midi + (oct as u8) * 12 + black_sem[local_wi] } else { 0 };
            let rb_active  = hb && active.contains(&midi_rb);

            let ws_style = if w_active { Style::default().bg(Color::Yellow).fg(Color::Black) }
                           else        { Style::default().bg(Color::White).fg(Color::Black) };
            let bk_active_sty = Style::default().bg(Color::Yellow).fg(Color::Black);
            let bk_sty        = Style::default().bg(Color::Black).fg(Color::White);

            let lc = if left_black { Span::styled("█", if lb_active { bk_active_sty } else { bk_sty }) }
                     else          { Span::styled(" ", ws_style) };
            let mc = if row == 3 {
                let label = if oct < num_oct { upper_black.get(local_wi).copied().unwrap_or(" ") } else { " " };
                Span::styled(label, ws_style)
            } else { Span::styled(" ", ws_style) };
            let rc = if hb { Span::styled("█", if rb_active { bk_active_sty } else { bk_sty }) }
                     else  { Span::styled(" ", ws_style) };
            s.push(lc); s.push(mc); s.push(rc); s.push(Span::raw("│"));
        }
        lines.push(Line::from(s));
    }

    // Black key label row
    {
        let mut s = vec![Span::raw("│")];
        for wi in 0..n_white {
            let oct = wi / 7; let local_wi = wi % 7;
            let hb = wi < n_white-1 && has_black[local_wi];

            let midi_w = if wi == n_white - 1 {
                base_midi + 24
            } else {
                base_midi + (oct as u8) * 12 + white_sem[local_wi]
            };
            let w_active  = active.contains(&midi_w);
            let midi_rb = if hb { base_midi + (oct as u8) * 12 + black_sem[local_wi] } else { 0 };
            let rb_active = hb && active.contains(&midi_rb);

            let ll = if local_wi > 0 && has_black[local_wi-1] {
                if oct == 0 { lower_black[local_wi-1] } else { upper_black[local_wi-1] }
            } else { "" };
            let rl = if hb { if oct == 0 { lower_black[local_wi] } else { upper_black[local_wi] } } else { "" };

            let ws_sty   = if w_active { Style::default().bg(Color::Yellow).fg(Color::Black) }
                           else        { Style::default().bg(Color::White).fg(Color::Black) };
            let bk_a_sty = Style::default().bg(Color::Yellow).fg(Color::Black).add_modifier(Modifier::BOLD);
            let bk_sty   = Style::default().bg(Color::Black).fg(Color::DarkGray);

            let lhb = local_wi > 0 && has_black[local_wi-1];
            let midi_la = if lhb { base_midi + (oct as u8) * 12 + black_sem[local_wi - 1] } else { 0 };
            let la  = lhb && active.contains(&midi_la);
            let lc  = if lhb { Span::styled(ll, if la { bk_a_sty } else { bk_sty }) } else { Span::styled(" ", ws_sty) };
            let mc  = Span::styled(" ", ws_sty);
            let rc  = if hb { Span::styled(rl, if rb_active { bk_a_sty } else { bk_sty }) } else { Span::styled(" ", ws_sty) };
            s.push(lc); s.push(mc); s.push(rc); s.push(Span::raw("│"));
        }
        lines.push(Line::from(s));
    }

    // Separator
    {
        let mut s = vec![Span::raw("│")];
        for wi in 0..n_white {
            let oct = wi / 7;
            let local_wi = wi % 7;
            let midi_w = if wi == n_white - 1 {
                base_midi + 24
            } else {
                base_midi + (oct as u8) * 12 + white_sem[local_wi]
            };
            let w_active = active.contains(&midi_w);
            let sty = if w_active { Style::default().bg(Color::Yellow).fg(Color::Black) }
                      else        { Style::default().bg(Color::White).fg(Color::Black) };
            let hbl = local_wi > 0 && has_black[local_wi-1];
            let hbr = wi < n_white-1 && has_black[local_wi];
            s.push(Span::styled(if hbl { "┘" } else { " " }, sty));
            s.push(Span::styled(" ", sty));
            s.push(Span::styled(if hbr { "└" } else { " " }, sty));
            s.push(Span::raw("│"));
        }
        lines.push(Line::from(s));
    }

    // White key labels
    {
        let mut s = vec![Span::raw("│")];
        for wi in 0..n_white {
            let oct = wi / 7; let local_wi = wi % 7;
            let midi_w = if wi == n_white - 1 {
                base_midi + 24
            } else {
                base_midi + (oct as u8) * 12 + white_sem[local_wi]
            };
            let w_active = active.contains(&midi_w);
            let sty = if w_active { Style::default().bg(Color::Yellow).fg(Color::Black).add_modifier(Modifier::BOLD) }
                      else        { Style::default().bg(Color::White).fg(Color::DarkGray) };
            let label = if wi == n_white-1 { "" } else if oct == 0 { lower_white[local_wi] } else { upper_white[local_wi] };
            s.push(Span::styled(format!("{:^3}", label), sty));
            s.push(Span::raw("│"));
        }
        lines.push(Line::from(s));
    }

    // Note names
    {
        let mut s = vec![Span::raw("│")];
        for wi in 0..n_white {
            let oct = wi / 7;
            let local_wi = wi % 7;
            let midi_w = if wi == n_white - 1 {
                base_midi + 24
            } else {
                base_midi + (oct as u8) * 12 + white_sem[local_wi]
            };
            let w_active = active.contains(&midi_w);
            let sty = if w_active { Style::default().bg(Color::Yellow).fg(Color::Black).add_modifier(Modifier::BOLD) }
                      else        { Style::default().bg(Color::White).fg(Color::Black) };
            let name = if wi == n_white-1 { "C" } else { note_names[local_wi] };
            s.push(Span::styled(format!("{:^3}", name), sty));
            s.push(Span::raw("│"));
        }
        lines.push(Line::from(s));
    }

    // Bottom border
    {
        let mut s = vec![Span::raw("└")];
        for i in 0..n_white { s.push(Span::raw("───")); if i < n_white-1 { s.push(Span::raw("┴")); } }
        s.push(Span::raw("┘"));
        lines.push(Line::from(s));
    }

    f.render_widget(Paragraph::new(lines), area);
}

// ── Melodic step sequencer ────────────────────────────────────────────────────

fn draw_synth_seq(f: &mut Frame, area: Rect, app: &App) {
    let focused = app.mode == AppMode::SynthSeq;
    let title = if focused {
        " ► Synth Seq — [←→] Cursor  [↑↓] BPM  [Enter/Space] Play  [Del] Clear  []] Steps  [-=] Vol  [[{] Oct "
    } else {
        " Synth Seq "
    };

    let (bpm, num_steps, current_step, playing, steps, volume) = {
        let s = app.synth.lock().unwrap();
        (s.bpm, s.sequencer.num_steps, s.sequencer.current_step,
         s.sequencer.playing, s.sequencer.steps.clone(), s.volume)
    };
    let cursor = app.seq_cursor;
    let mut lines: Vec<Line> = Vec::new();

    let (status_str, status_color) =
        if playing { ("▶ PLAYING", Color::Green) } else { ("■ STOPPED", Color::DarkGray) };
    lines.push(Line::from(vec![
        Span::styled("BPM: ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{:.0}", bpm), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled("Steps: ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{}", num_steps), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(status_str, Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled("Vol: ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{:.0}%", volume * 100.0), Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(format!("Oct:{}", app.base_octave), Style::default().fg(Color::DarkGray)),
    ]));

    let per_row = if num_steps <= 8 { 8 } else { 16 };
    for chunk_start in (0..num_steps).step_by(per_row) {
        let chunk_end = (chunk_start + per_row).min(num_steps);

        let mut nums = Vec::new();
        for i in chunk_start..chunk_end {
            let is_ph = playing && i == current_step;
            let is_cu = i == cursor;
            let sty = if is_ph && is_cu { Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD) }
                      else if is_ph     { Style::default().fg(Color::Black).bg(Color::Green) }
                      else if is_cu     { Style::default().fg(Color::Black).bg(Color::Yellow) }
                      else              { Style::default().fg(Color::DarkGray) };
            nums.push(Span::styled(format!("{:^5}", i + 1), sty));
        }
        lines.push(Line::from(nums));

        let mut cells = Vec::new();
        for i in chunk_start..chunk_end {
            let is_ph = playing && i == current_step;
            let is_cu = i == cursor;
            let cell = match steps[i] {
                Some(n) => format!("[{:<3}]", note_name(n)),
                None    => "[ · ]".to_string(),
            };
            let sty = if is_ph && is_cu   { Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD) }
                      else if is_ph       { Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD) }
                      else if is_cu       { Style::default().fg(Color::Black).bg(Color::Yellow) }
                      else if steps[i].is_some() { Style::default().fg(Color::White) }
                      else               { Style::default().fg(Color::DarkGray) };
            cells.push(Span::styled(cell, sty));
        }
        lines.push(Line::from(cells));
    }

    let note_disp = steps.get(cursor).copied().flatten()
        .map(|n| note_name(n)).unwrap_or_else(|| "·".to_string());
    lines.push(Line::from(vec![
        Span::styled("Cursor: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("step {}/{}  note: {}", cursor + 1, num_steps, note_disp),
            Style::default().fg(Color::White),
        ),
    ]));

    f.render_widget(
        Paragraph::new(lines).block(
            Block::default().title(title).borders(Borders::ALL)
                .border_style(if focused {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default().fg(Color::DarkGray)
                })
        ),
        area,
    );
}

// ── Melodic step sequencer 2 ──────────────────────────────────────────────────

fn draw_synth_seq2(f: &mut Frame, area: Rect, app: &App) {
    let focused = app.mode == AppMode::SynthSeq2;
    let title = if focused {
        " ► Synth Seq 2 — [←→] Cursor  [↑↓] BPM  [Enter/Space] Play  [Del] Clear  []] Steps  [F5] Wave  [-=] Vol  [[{] Oct "
    } else {
        " Synth Seq 2 "
    };

    let (bpm, num_steps, current_step, playing, steps, wave_name, volume2) = {
        let s = app.synth.lock().unwrap();
        (s.bpm, s.sequencer2.num_steps, s.sequencer2.current_step,
         s.sequencer2.playing, s.sequencer2.steps.clone(),
         s.wave_type2.name().to_string(), s.volume2)
    };
    let cursor = app.seq2_cursor;
    let mut lines: Vec<Line> = Vec::new();

    let (status_str, status_color) =
        if playing { ("▶ PLAYING", Color::Green) } else { ("■ STOPPED", Color::DarkGray) };
    lines.push(Line::from(vec![
        Span::styled("BPM: ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{:.0}", bpm), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled("Steps: ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{}", num_steps), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(status_str, Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled("Wave: ", Style::default().fg(Color::DarkGray)),
        Span::styled(wave_name, Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled("Vol: ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{:.0}%", volume2 * 100.0), Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(format!("Oct:{}", app.base_octave), Style::default().fg(Color::DarkGray)),
    ]));

    let per_row = if num_steps <= 8 { 8 } else { 16 };
    for chunk_start in (0..num_steps).step_by(per_row) {
        let chunk_end = (chunk_start + per_row).min(num_steps);

        let mut nums = Vec::new();
        for i in chunk_start..chunk_end {
            let is_ph = playing && i == current_step;
            let is_cu = i == cursor;
            let sty = if is_ph && is_cu { Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD) }
                      else if is_ph     { Style::default().fg(Color::Black).bg(Color::Green) }
                      else if is_cu     { Style::default().fg(Color::Black).bg(Color::Yellow) }
                      else              { Style::default().fg(Color::DarkGray) };
            nums.push(Span::styled(format!("{:^5}", i + 1), sty));
        }
        lines.push(Line::from(nums));

        let mut cells = Vec::new();
        for i in chunk_start..chunk_end {
            let is_ph = playing && i == current_step;
            let is_cu = i == cursor;
            let cell = match steps[i] {
                Some(n) => format!("[{:<3}]", note_name(n)),
                None    => "[ · ]".to_string(),
            };
            let sty = if is_ph && is_cu   { Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD) }
                      else if is_ph       { Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD) }
                      else if is_cu       { Style::default().fg(Color::Black).bg(Color::Yellow) }
                      else if steps[i].is_some() { Style::default().fg(Color::White) }
                      else               { Style::default().fg(Color::DarkGray) };
            cells.push(Span::styled(cell, sty));
        }
        lines.push(Line::from(cells));
    }

    let note_disp = steps.get(cursor).copied().flatten()
        .map(|n| note_name(n)).unwrap_or_else(|| "·".to_string());
    lines.push(Line::from(vec![
        Span::styled("Cursor: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("step {}/{}  note: {}", cursor + 1, num_steps, note_disp),
            Style::default().fg(Color::White),
        ),
    ]));

    f.render_widget(
        Paragraph::new(lines).block(
            Block::default().title(title).borders(Borders::ALL)
                .border_style(if focused {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default().fg(Color::DarkGray)
                })
        ),
        area,
    );
}

// ── Drum machine grid ─────────────────────────────────────────────────────────

fn drum_color(kind: DrumKind) -> Color {
    match kind {
        DrumKind::Kick      => Color::Red,
        DrumKind::Snare     => Color::Yellow,
        DrumKind::ClosedHat => Color::Cyan,
        DrumKind::OpenHat   => Color::Blue,
        DrumKind::Clap      => Color::Magenta,
        DrumKind::LowTom    => Color::Green,
        DrumKind::MidTom    => Color::LightGreen,
        DrumKind::HighTom   => Color::LightCyan,
    }
}

fn draw_drums(f: &mut Frame, area: Rect, app: &App) {
    let focused = app.mode == AppMode::Drums;
    let title = if focused {
        " ► Drum Machine — [↑↓] Track  [←→] Step  [Space] Toggle  [\\] Mute  [-=] Vol  []] Steps  [p/[] Prob  [e] Euclid "
    } else {
        " Drum Machine "
    };

    let (bpm, num_steps, current_step, playing, swing, tracks) = {
        let s = app.synth.lock().unwrap();
        let dm = &s.drum_machine;
        let tracks: Vec<(DrumKind, Vec<u8>, bool, f32)> =
            dm.tracks.iter().map(|t| (t.kind, t.steps.clone(), t.muted, t.volume)).collect();
        (s.bpm, dm.num_steps, dm.current_step, dm.playing, dm.swing, tracks)
    };
    let sel_track = app.drum_track;
    let sel_step  = app.drum_step;

    let mut lines: Vec<Line> = Vec::new();

    let swing_pct = (swing * 100.0).round() as u32;
    let (status_str, status_color) =
        if playing { ("▶ PLAYING", Color::Green) } else { ("■ STOPPED", Color::DarkGray) };
    lines.push(Line::from(vec![
        Span::styled("BPM: ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{:.0}", bpm), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled("Steps: ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{}", num_steps), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(status_str, Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled("Swing: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}%", swing_pct),
            if swing_pct > 0 {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        ),
    ]));

    {
        let mut s = vec![Span::styled("              ", Style::default())];
        for i in 0..num_steps {
            let is_ph = playing && i == current_step;
            let label = if i % 4 == 0 { format!("{:>2}", i + 1) } else { " .".to_string() };
            let sty = if is_ph { Style::default().fg(Color::Green).add_modifier(Modifier::BOLD) }
                      else     { Style::default().fg(Color::DarkGray) };
            s.push(Span::styled(label, sty));
        }
        lines.push(Line::from(s));
    }

    for (ti, (kind, steps, muted, volume)) in tracks.iter().enumerate() {
        let is_selected = ti == sel_track;
        let track_color = drum_color(*kind);
        let vol_pct = (volume * 100.0).round() as u32;

        let mute_char  = if *muted { 'M' } else { '·' };
        let name_style = if is_selected && !muted {
            Style::default().fg(track_color).add_modifier(Modifier::BOLD)
        } else if is_selected {
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)
        } else if *muted {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(track_color)
        };
        let mute_style = Style::default().fg(Color::DarkGray);
        let vol_style = if is_selected && focused {
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let mut row: Vec<Span> = vec![
            Span::styled(format!(" {:5}", kind.name()), name_style),
            Span::styled("[", Style::default().fg(Color::DarkGray)),
            Span::styled(mute_char.to_string(), mute_style),
            Span::styled("]", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{:3}%", vol_pct), vol_style),
            Span::styled("│", Style::default().fg(Color::DarkGray)),
        ];

        for i in 0..num_steps {
            let prob    = steps.get(i).copied().unwrap_or(0);
            let active  = prob > 0;
            let is_ph   = playing && i == current_step;
            let is_cu   = is_selected && i == sel_step;

            let cell_char = match prob {
                0       => "·",
                1..=33  => "░",
                34..=66 => "▒",
                67..=99 => "▓",
                _       => "█",
            };

            let sty = if is_ph && is_cu {
                Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else if is_ph {
                Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD)
            } else if is_cu {
                Style::default().fg(Color::Black).bg(Color::Yellow)
            } else if active && !muted {
                Style::default().fg(track_color).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            if i > 0 && i % 4 == 0 {
                row.push(Span::styled("┆", Style::default().fg(Color::DarkGray)));
            }
            row.push(Span::styled(format!("{} ", cell_char), sty));
        }

        lines.push(Line::from(row));
    }

    f.render_widget(
        Paragraph::new(lines).block(
            Block::default().title(title).borders(Borders::ALL)
                .border_style(if focused {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default().fg(Color::DarkGray)
                })
        ),
        area,
    );
}

// ── Effects panel ─────────────────────────────────────────────────────────────

/// 8-character progress bar.
fn pbar(v: f32, max: f32) -> String {
    let pct    = (v / max).clamp(0.0, 1.0);
    let filled = ((pct * 8.0).round() as usize).min(8);
    format!("{}{}", "█".repeat(filled), "░".repeat(8 - filled))
}

/// 4-character progress bar for send levels (0.0–1.0).
fn pbar4(v: f32) -> String {
    let filled = ((v.clamp(0.0, 1.0) * 4.0).round() as usize).min(4);
    format!("{}{}", "█".repeat(filled), "░".repeat(4 - filled))
}

fn draw_effects(f: &mut Frame, area: Rect, app: &App) {
    let focused = app.mode == AppMode::Effects;
    let title = if focused {
        " ► Effects — [↑↓] Select  [←→] Param  [-=] Adjust  [Enter] On/Off  [Space] Route 0↔100% "
    } else {
        " Effects "
    };

    // Snapshot all effect params + routing in one lock acquisition
    let (rev_en, rev_room, rev_damp, rev_mix,
         dly_en, dly_time, dly_feed, dly_mix,
         dst_en, dst_drv, dst_tone, dst_lvl,
         s1_rev, s2_rev, dr_rev,
         s1_dly, s2_dly, dr_dly,
         s1_dst, s2_dst, dr_dst,
         sc_en, sc_depth, sc_rel, sc_s1, sc_s2) = {
        let s = app.synth.lock().unwrap();
        (s.reverb.enabled, s.reverb.room_size, s.reverb.damping, s.reverb.mix,
         s.delay.enabled,  s.delay.time_ms,    s.delay.feedback,  s.delay.mix,
         s.distortion.enabled, s.distortion.drive, s.distortion.tone, s.distortion.level,
         s.fx_routing.s1_reverb, s.fx_routing.s2_reverb, s.fx_routing.dr_reverb,
         s.fx_routing.s1_delay,  s.fx_routing.s2_delay,  s.fx_routing.dr_delay,
         s.fx_routing.s1_dist,   s.fx_routing.s2_dist,   s.fx_routing.dr_dist,
         s.sidechain.enabled, s.sidechain.depth, s.sidechain.release_ms,
         s.sidechain.duck_s1, s.sidechain.duck_s2)
    };

    let sel = app.effects_sel;
    let par = app.effects_param;

    // Build one effect row (params 0-2 + routing sends 3-5)
    let make_row = |fi: usize, enabled: bool, color: Color, name: &str,
                    labels: &[&str; 3], vals: &[f32; 3], maxes: &[f32; 3], disps: &[String; 3],
                    sends: &[f32; 3]| -> Line {
        let is_sel = fi == sel;
        let on_str   = if enabled { "[ON ] " } else { "[OFF] " };
        let on_style = if enabled { Style::default().fg(Color::Green) }
                       else       { Style::default().fg(Color::DarkGray) };
        let name_sty = if is_sel && enabled {
            Style::default().fg(color).add_modifier(Modifier::BOLD)
        } else if is_sel {
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)
        } else if enabled {
            Style::default().fg(color)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let mut spans: Vec<Span> = vec![
            Span::styled(on_str, on_style),
            Span::styled(name.to_string(), name_sty),
            Span::raw("  "),
        ];

        // Params 0-2: effect-specific knobs
        for pi in 0..3 {
            let is_sp = is_sel && pi == par;
            let bar   = pbar(vals[pi], maxes[pi]);
            let sty   = if is_sp && focused {
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
            } else if !enabled {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::Gray)
            };
            spans.push(Span::styled(
                format!("{}: [{}] {:>5}  ", labels[pi], bar, disps[pi]),
                sty,
            ));
        }

        // Params 3-5: routing send levels (S1, S2, DR)
        for (ri, (&send, rlbl)) in sends.iter().zip(["S1","S2","DR"].iter()).enumerate() {
            let pi = ri + 3;
            let is_sp = is_sel && pi == par;
            let pct = (send * 100.0).round() as u32;
            let sty = if is_sp && focused {
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
            } else if send > 0.0 && enabled {
                Style::default().fg(Color::Gray)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            spans.push(Span::styled(
                format!("{}:[{}]{:>3}%  ", rlbl, pbar4(send), pct),
                sty,
            ));
        }

        Line::from(spans)
    };

    let rev_d = [format!("{:.0}%",  rev_room * 100.0),
                 format!("{:.0}%",  rev_damp * 100.0),
                 format!("{:.0}%",  rev_mix  * 100.0)];
    let dly_d = [format!("{:.0}ms", dly_time),
                 format!("{:.0}%",  dly_feed * 100.0),
                 format!("{:.0}%",  dly_mix  * 100.0)];
    let dst_d = [format!("{:.1}x",  dst_drv),
                 format!("{:.0}%",  dst_tone * 100.0),
                 format!("{:.0}%",  dst_lvl  * 100.0)];
    let sc_d  = [format!("{:.0}%",  sc_depth * 100.0),
                 format!("{:.0}ms", sc_rel),
                 "---".to_string()];

    let lines = vec![
        make_row(0, rev_en, Color::Blue,    "REVERB ", &["Room","Damp","Mix "],
                 &[rev_room, rev_damp, rev_mix], &[1.0, 1.0, 1.0], &rev_d,
                 &[s1_rev, s2_rev, dr_rev]),
        make_row(1, dly_en, Color::Green,   "DELAY  ", &["Time","Feed","Mix "],
                 &[dly_time, dly_feed, dly_mix], &[1000.0, 0.95, 1.0], &dly_d,
                 &[s1_dly, s2_dly, dr_dly]),
        make_row(2, dst_en, Color::Red,     "DISTORT", &["Drv ","Tone","Lvl "],
                 &[dst_drv,  dst_tone, dst_lvl],  &[10.0,  1.0,  1.0], &dst_d,
                 &[s1_dst, s2_dst, dr_dst]),
        make_row(3, sc_en,  Color::Magenta, "SIDECHN", &["Dpth","Rel ","--- "],
                 &[sc_depth, sc_rel, 0.0], &[1.0, 500.0, 1.0], &sc_d,
                 &[sc_s1 as u8 as f32, sc_s2 as u8 as f32, 0.0]),
    ];

    f.render_widget(
        Paragraph::new(lines).block(
            Block::default().title(title).borders(Borders::ALL)
                .border_style(if focused {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default().fg(Color::DarkGray)
                })
        ),
        area,
    );
}

// ── Status bar ────────────────────────────────────────────────────────────────

fn draw_status(f: &mut Frame, area: Rect, app: &App) {
    let wave    = app.wave_name();
    let vol     = app.volume();
    let bpm     = { app.synth.lock().unwrap().bpm };
    let notes   = app.active_note_names();
    let notes_s = if notes.is_empty() { "—".to_string() } else { notes.join(" ") };
    let extra   = if app.status_msg.is_empty() { String::new() } else { format!("  │  {}", app.status_msg) };

    let text = vec![
        Line::from(vec![
            Span::styled("Wave: ",   Style::default().fg(Color::DarkGray)),
            Span::styled(&wave,      Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("  │  "),
            Span::styled("BPM: ",    Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{:.0}", bpm), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("  │  "),
            Span::styled("Vol: ",    Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{:.0}%", vol * 100.0),
                         Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
            Span::styled(&extra,     Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            Span::styled("Playing: ", Style::default().fg(Color::DarkGray)),
            Span::styled(notes_s,     Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]),
    ];

    f.render_widget(
        Paragraph::new(text)
            .block(Block::default().title(" Status ").borders(Borders::ALL))
            .wrap(Wrap { trim: false }),
        area,
    );
}

// ── Oscilloscope ──────────────────────────────────────────────────────────────

fn braille_bit(col: usize, row: usize) -> u8 {
    match (col, row) {
        (0, 0) => 0x01, (0, 1) => 0x02, (0, 2) => 0x04, (0, 3) => 0x40,
        (1, 0) => 0x08, (1, 1) => 0x10, (1, 2) => 0x20, (1, 3) => 0x80,
        _ => 0,
    }
}

fn draw_oscilloscope(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default().title(" Scope ").borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let (buf, pos) = {
        let s = app.synth.lock().unwrap();
        (s.scope_buf.clone(), s.scope_pos)
    };
    let w = inner.width as usize;
    let h = inner.height as usize;
    if w == 0 || h == 0 { return; }

    let n = (w * 2).min(buf.len());
    let start = pos.wrapping_sub(n) % buf.len();
    let samples: Vec<f32> = (0..n).map(|i| buf[(start + i) % buf.len()]).collect();

    let mut lines = Vec::with_capacity(h);
    for row in 0..h {
        let mut spans = Vec::with_capacity(w);
        for col in 0..w {
            let mut bits = 0u8;
            for dc in 0..2usize {
                let si = col * 2 + dc;
                if si >= samples.len() { continue; }
                let sv = samples[si].clamp(-1.0, 1.0);
                let y = ((1.0 - sv) * 0.5 * (h * 4) as f32) as usize;
                let y = y.min(h * 4 - 1);
                if y / 4 == row { bits |= braille_bit(dc, y % 4); }
            }
            let ch = char::from_u32(0x2800 + bits as u32).unwrap_or(' ');
            let color = if bits != 0 { Color::Cyan } else { Color::DarkGray };
            spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

// ── Unified help panel ────────────────────────────────────────────────────────

fn draw_help(f: &mut Frame, area: Rect, app: &App) {
    let w = Style::default().fg(Color::White);
    let d = Style::default().fg(Color::DarkGray);

    let global = Line::from(vec![
        Span::styled("[Tab/F2] ", w), Span::raw("Cycle focus  │  "),
        Span::styled("[F1] ",     w), Span::raw("Waveform  │  "),
        Span::styled("[F3] ",     w), Span::raw("Drum play/stop  │  "),
        Span::styled("[PgUp/Dn] ",w), Span::raw("BPM  │  "),
        Span::styled("[Esc] ",    w), Span::raw("Quit"),
    ]);

    let focus_line = match app.mode {
        AppMode::Play => Line::from(vec![
            Span::styled("Keys: ", d),
            Span::raw("Z X C V B N M  (white)  S D G H J  (black)  │  upper row: Q-P / 2-0"),
        ]),
        AppMode::SynthSeq => Line::from(vec![
            Span::styled("Piano keys: ", d),
            Span::raw("set note at cursor (advances)  │  "),
            Span::styled("[Enter/Space] ", w), Span::raw("Play/Pause  │  "),
            Span::styled("[Del] ",   w), Span::raw("Clear  │  "),
            Span::styled("[]] ",     w), Span::raw("Cycle steps  │  "),
            Span::styled("[-=] ",    w), Span::raw("Vol  │  "),
            Span::styled("[[{] ",    w), Span::raw("Oct down/up"),
        ]),
        AppMode::SynthSeq2 => Line::from(vec![
            Span::styled("Piano keys: ", d),
            Span::raw("set note at cursor (advances)  │  "),
            Span::styled("[Enter/Space] ", w), Span::raw("Play/Pause  │  "),
            Span::styled("[Del] ",   w), Span::raw("Clear  │  "),
            Span::styled("[]] ",     w), Span::raw("Cycle steps  │  "),
            Span::styled("[F5] ",    w), Span::raw("Wave  │  "),
            Span::styled("[-=] ",    w), Span::raw("Vol  │  "),
            Span::styled("[[{] ",    w), Span::raw("Oct down/up"),
        ]),
        AppMode::Drums => Line::from(vec![
            Span::styled("Preview: ", d),
            Span::styled("Z",  Style::default().fg(Color::Red)),     Span::raw(" Kick  "),
            Span::styled("X",  Style::default().fg(Color::Yellow)),  Span::raw(" Snare  "),
            Span::styled("C",  Style::default().fg(Color::Cyan)),    Span::raw(" C-Hat  "),
            Span::styled("V",  Style::default().fg(Color::Blue)),    Span::raw(" O-Hat  "),
            Span::styled("B",  Style::default().fg(Color::Magenta)), Span::raw(" Clap  "),
            Span::styled("N",  Style::default().fg(Color::Green)),   Span::raw(" L.Tom  "),
            Span::styled("M",  Style::default().fg(Color::LightGreen)), Span::raw(" M.Tom  "),
            Span::styled(",",  Style::default().fg(Color::LightCyan)),  Span::raw(" H.Tom  │  "),
            Span::styled("[Enter] ", w), Span::raw("Play  │  "),
            Span::styled("[\\ ] ", w),  Span::raw("Mute  │  "),
            Span::styled("[Del] ",  w), Span::raw("Clear  │  "),
            Span::styled("[p/[] ", w),  Span::raw("Prob +/-25%  │  "),
            Span::styled("[e] ",    w), Span::raw("Euclidean fill"),
        ]),
        AppMode::Effects => Line::from(vec![
            Span::styled("[↑↓] ", w), Span::raw("Select effect (row 4=Sidechain)  │  "),
            Span::styled("[←→] ", w), Span::raw("Param (col 1-3) or send (col 4-6)  │  "),
            Span::styled("[-=] ", w), Span::raw("Adjust  │  "),
            Span::styled("[Enter] ", w), Span::raw("On/Off  │  "),
            Span::styled("[Space col 4-6] ", w), Span::raw("Route/Duck S1/S2 0↔100%"),
        ]),
    };

    f.render_widget(
        Paragraph::new(vec![global, focus_line])
            .block(Block::default().title(" Help ").borders(Borders::ALL))
            .style(Style::default().fg(Color::DarkGray)),
        area,
    );
}
