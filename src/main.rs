mod app;
mod audio;
mod drums;
mod effects;
mod sequencer;
mod synth;
mod ui;

use anyhow::Result;
use app::{App, AppMode};
use audio::AudioEngine;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind,
        KeyboardEnhancementFlags, KeyModifiers, PopKeyboardEnhancementFlags,
        PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, supports_keyboard_enhancement, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{io, sync::{Arc, Mutex}, time::Duration};
use synth::Synth;

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();

    let enhanced = supports_keyboard_enhancement().unwrap_or(false);
    if enhanced {
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture,
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                    | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES))?;
    } else {
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    }

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run(&mut terminal, enhanced);

    disable_raw_mode()?;
    if enhanced {
        execute!(terminal.backend_mut(),
            PopKeyboardEnhancementFlags, LeaveAlternateScreen, DisableMouseCapture)?;
    } else {
        execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    }
    terminal.show_cursor()?;
    if let Err(e) = result { eprintln!("Error: {:?}", e); }
    Ok(())
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, enhanced: bool) -> Result<()> {
    let synth  = Arc::new(Mutex::new(Synth::new(44100.0)));
    let _audio = AudioEngine::new(Arc::clone(&synth))?;
    let mut app = App::new(Arc::clone(&synth));

    loop {
        if !enhanced { app.tick_fallback_release(); }
        app.refresh_active_notes();
        terminal.draw(|f| ui::draw(f, &app, enhanced))?;

        if event::poll(Duration::from_millis(16))? {
            match event::read()? {
                Event::Key(key) => {
                    // ── Key release (enhanced mode only) ──────────────────
                    if key.kind == KeyEventKind::Release {
                        if app.mode == AppMode::Play {
                            if let KeyCode::Char(c) = key.code { app.key_release(c); }
                        }
                        continue;
                    }

                    // ── Key repeat ────────────────────────────────────────
                    if key.kind == KeyEventKind::Repeat {
                        match key.code {
                            // Global BPM
                            KeyCode::PageUp   => app.bpm_up(),
                            KeyCode::PageDown => app.bpm_down(),

                            // Effects focus: navigation + param adjust (no Space repeat)
                            KeyCode::Up    if app.mode == AppMode::Effects => app.effects_sel_up(),
                            KeyCode::Down  if app.mode == AppMode::Effects => app.effects_sel_down(),
                            KeyCode::Left  if app.mode == AppMode::Effects => app.effects_param_left(),
                            KeyCode::Right if app.mode == AppMode::Effects => app.effects_param_right(),
                            KeyCode::Char('=') if app.mode == AppMode::Effects => app.effects_param_inc(),
                            KeyCode::Char('-') if app.mode == AppMode::Effects => app.effects_param_dec(),

                            // Drums focus: navigation + drum vol repeat
                            KeyCode::Up    if app.mode == AppMode::Drums => app.drum_track_up(),
                            KeyCode::Down  if app.mode == AppMode::Drums => app.drum_track_down(),
                            KeyCode::Left  if app.mode == AppMode::Drums => app.drum_step_left(),
                            KeyCode::Right if app.mode == AppMode::Drums => app.drum_step_right(),
                            KeyCode::Char('=') if app.mode == AppMode::Drums => app.drum_vol_up(),
                            KeyCode::Char('-') if app.mode == AppMode::Drums => app.drum_vol_down(),

                            // SynthSeq2 focus: cursor + BPM
                            KeyCode::Left  if app.mode == AppMode::SynthSeq2 => app.seq2_cursor_left(),
                            KeyCode::Right if app.mode == AppMode::SynthSeq2 => app.seq2_cursor_right(),
                            KeyCode::Up    if app.mode == AppMode::SynthSeq2 => app.bpm_up(),
                            KeyCode::Down  if app.mode == AppMode::SynthSeq2 => app.bpm_down(),

                            // SynthSeq focus: cursor + BPM
                            KeyCode::Left  if app.mode == AppMode::SynthSeq => app.seq_cursor_left(),
                            KeyCode::Right if app.mode == AppMode::SynthSeq => app.seq_cursor_right(),
                            KeyCode::Up    if app.mode == AppMode::SynthSeq => app.bpm_up(),
                            KeyCode::Down  if app.mode == AppMode::SynthSeq => app.bpm_down(),

                            // Keyboard focus: octave + volume
                            KeyCode::Left  => app.octave_down(),
                            KeyCode::Right => app.octave_up(),
                            KeyCode::Up    => app.volume_up(),
                            KeyCode::Down  => app.volume_down(),

                            _ => {
                                if let KeyCode::Char(c) = key.code {
                                    if app.mode == AppMode::Play { app.key_press_fallback(c); }
                                }
                            }
                        }
                        continue;
                    }

                    // ── Key press ─────────────────────────────────────────
                    match key.code {
                        // Global quit
                        KeyCode::Esc => break,
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,

                        // Global: cycle focus, waveform, drum play, BPM
                        KeyCode::Tab          => app.toggle_mode(),
                        KeyCode::F(2)         => app.toggle_mode(),
                        KeyCode::F(1)         => app.cycle_wave(),
                        KeyCode::F(3)         => app.drum_toggle_play(),
                        KeyCode::PageUp       => app.bpm_up(),
                        KeyCode::PageDown     => app.bpm_down(),

                        // ── Effects focus ─────────────────────────────────
                        KeyCode::Up    if app.mode == AppMode::Effects => app.effects_sel_up(),
                        KeyCode::Down  if app.mode == AppMode::Effects => app.effects_sel_down(),
                        KeyCode::Left  if app.mode == AppMode::Effects => app.effects_param_left(),
                        KeyCode::Right if app.mode == AppMode::Effects => app.effects_param_right(),
                        KeyCode::Char('=') if app.mode == AppMode::Effects => app.effects_param_inc(),
                        KeyCode::Char('-') if app.mode == AppMode::Effects => app.effects_param_dec(),
                        KeyCode::Char(' ') if app.mode == AppMode::Effects => app.effects_toggle(),

                        // ── Drums focus ───────────────────────────────────
                        KeyCode::Up    if app.mode == AppMode::Drums => app.drum_track_up(),
                        KeyCode::Down  if app.mode == AppMode::Drums => app.drum_track_down(),
                        KeyCode::Left  if app.mode == AppMode::Drums => app.drum_step_left(),
                        KeyCode::Right if app.mode == AppMode::Drums => app.drum_step_right(),
                        KeyCode::Enter if app.mode == AppMode::Drums => app.drum_toggle_play(),
                        KeyCode::Backspace | KeyCode::Delete if app.mode == AppMode::Drums => app.drum_clear_step(),
                        KeyCode::Char(' ')  if app.mode == AppMode::Drums => app.drum_toggle_step(),
                        KeyCode::Char(']')  if app.mode == AppMode::Drums => app.drum_cycle_steps(),
                        KeyCode::Char('\\') if app.mode == AppMode::Drums => app.drum_toggle_mute(),
                        KeyCode::Char('=')  if app.mode == AppMode::Drums => app.drum_vol_up(),
                        KeyCode::Char('-')  if app.mode == AppMode::Drums => app.drum_vol_down(),

                        // ── SynthSeq2 focus ───────────────────────────────
                        KeyCode::Left  if app.mode == AppMode::SynthSeq2 => app.seq2_cursor_left(),
                        KeyCode::Right if app.mode == AppMode::SynthSeq2 => app.seq2_cursor_right(),
                        KeyCode::Up    if app.mode == AppMode::SynthSeq2 => app.bpm_up(),
                        KeyCode::Down  if app.mode == AppMode::SynthSeq2 => app.bpm_down(),
                        KeyCode::Char(' ') if app.mode == AppMode::SynthSeq2 => app.seq2_toggle_play(),
                        KeyCode::Backspace | KeyCode::Delete if app.mode == AppMode::SynthSeq2 => app.seq2_clear_step(),
                        KeyCode::Char(']') if app.mode == AppMode::SynthSeq2 => app.seq2_cycle_steps(),
                        KeyCode::F(5)      if app.mode == AppMode::SynthSeq2 => app.cycle_wave2(),

                        // ── SynthSeq focus ────────────────────────────────
                        KeyCode::Left  if app.mode == AppMode::SynthSeq => app.seq_cursor_left(),
                        KeyCode::Right if app.mode == AppMode::SynthSeq => app.seq_cursor_right(),
                        KeyCode::Up    if app.mode == AppMode::SynthSeq => app.bpm_up(),
                        KeyCode::Down  if app.mode == AppMode::SynthSeq => app.bpm_down(),
                        KeyCode::Char(' ') if app.mode == AppMode::SynthSeq => app.seq_toggle_play(),
                        KeyCode::Backspace | KeyCode::Delete if app.mode == AppMode::SynthSeq => app.seq_clear_step(),
                        KeyCode::Char(']') if app.mode == AppMode::SynthSeq => app.seq_cycle_steps(),

                        // ── Keyboard focus ────────────────────────────────
                        KeyCode::Left  => app.octave_down(),
                        KeyCode::Right => app.octave_up(),
                        KeyCode::Up    => app.volume_up(),
                        KeyCode::Down  => app.volume_down(),

                        // ── Piano / drum preview / sequencer note keys ────
                        KeyCode::Char(c) => match app.mode {
                            AppMode::Play      => {
                                if enhanced { app.key_press(c); } else { app.key_press_fallback(c); }
                            }
                            AppMode::SynthSeq  => app.seq_set_note(c),
                            AppMode::SynthSeq2 => app.seq2_set_note(c),
                            AppMode::Drums     => app.drum_preview(c),
                            AppMode::Effects   => {}
                        },

                        _ => {}
                    }
                }
                Event::FocusLost => { app.release_all(); }
                _ => {}
            }
        }
        if app.should_quit { break; }
    }

    app.release_all();
    Ok(())
}
