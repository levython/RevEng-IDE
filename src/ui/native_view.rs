//! Native analysis panel — displays ELF headers, symbols, and disassembly.

use crate::native::elf::ElfInfo;
use crate::ui::theme::Theme;

pub enum NativeAction {
    None,
    AutoPatchSsl,
    FindReplaceBytes(String, String),
}

pub struct NativeView;

impl NativeView {
    /// Render the native analysis view.
    /// Returns the action requested by the user, if any.
    pub fn render(
        ui: &mut egui::Ui,
        info: &ElfInfo,
        instructions: &[crate::native::disasm::DisasmInstruction],
        find_hex: &mut String,
        replace_hex: &mut String,
    ) -> NativeAction {
        let theme = Theme::current(ui);
        let mut action = NativeAction::None;

        ui.vertical(|ui| {
            // ── Library type badge ──────────────────────────────────────────
            ui.horizontal(|ui| {
                if info.is_flutter_engine {
                    ui.label(
                        egui::RichText::new("Flutter Engine  libflutter.so")
                            .strong()
                            .color(egui::Color32::from_rgb(116, 199, 236)) // sapphire
                            .size(theme.font_ui),
                    );
                } else if info.is_dart_snapshot {
                    ui.label(
                        egui::RichText::new("Dart AOT Snapshot  libapp.so")
                            .strong()
                            .color(egui::Color32::from_rgb(203, 166, 247)) // mauve
                            .size(theme.font_ui),
                    );
                    ui.separator();
                    ui.label(
                        egui::RichText::new("Contains compiled Dart code (ARM64)")
                            .color(theme.text_muted)
                            .italics()
                            .size(theme.font_small),
                    );
                }
            });
            ui.add_space(6.0);

            // ── ELF Summary ────────────────────────────────────────────────
            egui::CollapsingHeader::new(
                egui::RichText::new("ELF Summary").strong().color(theme.text_accent),
            )
            .default_open(true)
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(crate::native::elf::ElfParser::summary(info))
                        .monospace()
                        .size(theme.font_small)
                        .color(theme.text_muted),
                );
                ui.add_space(4.0);
                egui::Grid::new("elf_summary_grid").striped(true).show(ui, |ui| {
                    let lbl = |s: &str| egui::RichText::new(s).color(theme.text_secondary);
                    let val = |s: &str| egui::RichText::new(s).color(theme.text_primary);
                    ui.label(lbl("Machine:")); ui.label(val(&info.machine)); ui.end_row();
                    ui.label(lbl("Type:"));    ui.label(val(&info.elf_type)); ui.end_row();
                    ui.label(lbl("Path:"));    ui.label(val(&info.path)); ui.end_row();
                    ui.label(lbl("Entry:"));   ui.label(val(&format!("{:#x}", info.entry_point))); ui.end_row();
                    ui.label(lbl("Sections:")); ui.label(val(&info.sections.len().to_string())); ui.end_row();
                    ui.label(lbl("Symbols:")); ui.label(val(&info.symbols.len().to_string())); ui.end_row();
                    if !info.dynamic_libs.is_empty() {
                        ui.label(lbl("Linked libs:"));
                        ui.label(val(&info.dynamic_libs.join(", ")));
                        ui.end_row();
                    }
                });
            });

            ui.add_space(8.0);

            // ── Sections ─────────────────────────────────────────────────
            egui::CollapsingHeader::new(
                egui::RichText::new(format!("Sections ({})", info.sections.len()))
                    .strong().color(theme.text_accent),
            )
            .default_open(false)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                    egui::Grid::new("sections_grid").striped(true).show(ui, |ui| {
                        ui.label(egui::RichText::new("Name").color(theme.text_secondary).size(theme.font_small));
                        ui.label(egui::RichText::new("Type").color(theme.text_secondary).size(theme.font_small));
                        ui.label(egui::RichText::new("Addr").color(theme.text_secondary).size(theme.font_small));
                        ui.label(egui::RichText::new("Size").color(theme.text_secondary).size(theme.font_small));
                        ui.end_row();
                        for sec in &info.sections {
                            // Highlight Dart-specific sections
                            let color = if sec.name.contains("kDart") || sec.name.contains("Dart") {
                                egui::Color32::from_rgb(203, 166, 247)
                            } else {
                                theme.text_primary
                            };
                            ui.label(egui::RichText::new(&sec.name).color(color).font(egui::FontId::monospace(theme.font_small)));
                            ui.label(egui::RichText::new(&sec.section_type).color(theme.text_muted).font(egui::FontId::monospace(theme.font_small)));
                            ui.label(egui::RichText::new(format!("{:#x}", sec.addr)).color(theme.disasm_address).font(egui::FontId::monospace(theme.font_small)));
                            ui.label(egui::RichText::new(format!("{:#x}", sec.size)).color(theme.text_muted).font(egui::FontId::monospace(theme.font_small)));
                            ui.end_row();
                        }
                    });
                });
            });

            ui.add_space(8.0);

            // ── Binary Patching ────────────────────────────────────────────
            egui::CollapsingHeader::new(
                egui::RichText::new("Binary Patching").strong().color(theme.text_accent),
            )
            .default_open(true)
            .show(ui, |ui| {
                if info.is_flutter_engine {
                    ui.horizontal(|ui| {
                        let btn = ui.add(
                            egui::Button::new(
                                egui::RichText::new("Auto-Patch Flutter SSL")
                                    .size(theme.font_small)
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(30, 102, 245)),
                        );
                        if btn.clicked() {
                            action = NativeAction::AutoPatchSsl;
                        }
                        ui.label(
                            egui::RichText::new("Patches libflutter.so ssl_verify to return true")
                                .size(theme.font_small)
                                .color(theme.text_muted),
                        );
                    });
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("A .so.bak backup is created before patching.")
                            .size(theme.font_small)
                            .color(theme.text_muted),
                    );
                    ui.label(
                        egui::RichText::new("After patching, rebuild the APK from the toolbar and reinstall.")
                            .size(theme.font_small)
                            .color(theme.text_muted),
                    );
                } else if info.is_dart_snapshot {
                    ui.label(
                        egui::RichText::new("Dart AOT snapshot. Use Frida for dynamic analysis, or blutter for static reconstruction.")
                            .size(theme.font_small)
                            .color(theme.text_muted),
                    );
                } else {
                    ui.label(
                        egui::RichText::new("Open libflutter.so for SSL bypass patching.")
                            .size(theme.font_small)
                            .color(theme.text_muted),
                    );
                }

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Find Hex:")
                            .size(theme.font_small)
                            .color(theme.text_secondary)
                    );
                    ui.add(egui::TextEdit::singleline(find_hex).hint_text("e.g. 01 02 A3"));
                    
                    ui.add_space(8.0);
                    
                    ui.label(
                        egui::RichText::new("Replace Hex:")
                            .size(theme.font_small)
                            .color(theme.text_secondary)
                    );
                    ui.add(egui::TextEdit::singleline(replace_hex).hint_text("e.g. 01 02 A4"));
                    
                    let is_valid = !find_hex.trim().is_empty() && !replace_hex.trim().is_empty();
                    let btn = ui.add_enabled(is_valid, egui::Button::new(
                        egui::RichText::new("Replace All")
                            .size(theme.font_small)
                            .color(if is_valid { theme.text_primary } else { theme.text_muted }),
                    ));
                    if btn.clicked() {
                        action = NativeAction::FindReplaceBytes(find_hex.clone(), replace_hex.clone());
                    }
                });
            });

            ui.add_space(8.0);

            // ── Symbols (imports + exports) ────────────────────────────────
            egui::CollapsingHeader::new(
                egui::RichText::new(format!("Exports ({})", info.exports.len()))
                    .strong().color(theme.text_accent),
            )
            .default_open(false)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                    egui::Grid::new("export_grid").striped(true).show(ui, |ui| {
                        for sym in &info.exports {
                            ui.label(
                                egui::RichText::new(format!("{:#010x}", sym.addr))
                                    .font(egui::FontId::monospace(theme.font_small))
                                    .color(theme.disasm_address),
                            );
                            let display = sym.demangled.as_deref().unwrap_or(&sym.name);
                            let color = if sym.name.contains("kDart") || sym.name.contains("Dart") {
                                egui::Color32::from_rgb(203, 166, 247)
                            } else {
                                theme.text_primary
                            };
                            ui.label(egui::RichText::new(display).color(color).size(theme.font_small));
                            ui.end_row();
                        }
                    });
                });
            });

            egui::CollapsingHeader::new(
                egui::RichText::new(format!("Imports ({})", info.imports.len()))
                    .strong().color(theme.text_accent),
            )
            .default_open(false)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                    egui::Grid::new("import_grid").striped(true).show(ui, |ui| {
                        for sym in &info.imports {
                            let display = sym.demangled.as_deref().unwrap_or(&sym.name);
                            ui.label(egui::RichText::new(display).color(theme.text_secondary).size(theme.font_small));
                            ui.end_row();
                        }
                    });
                });
            });

            ui.add_space(8.0);

            // ── Raw Symbol Table ─────────────────────────────────────────
            egui::CollapsingHeader::new(
                egui::RichText::new(format!("Symbol Table ({})", info.symbols.len()))
                    .strong().color(theme.text_accent),
            )
            .default_open(false)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                    egui::Grid::new("symbols_grid").striped(true).show(ui, |ui| {
                        ui.label(egui::RichText::new("Addr").color(theme.text_secondary).size(theme.font_small));
                        ui.label(egui::RichText::new("Size").color(theme.text_secondary).size(theme.font_small));
                        ui.label(egui::RichText::new("Bind").color(theme.text_secondary).size(theme.font_small));
                        ui.label(egui::RichText::new("Kind").color(theme.text_secondary).size(theme.font_small));
                        ui.label(egui::RichText::new("Name").color(theme.text_secondary).size(theme.font_small));
                        ui.end_row();

                        for sym in info.symbols.iter().take(2000) {
                            ui.label(
                                egui::RichText::new(format!("{:#010x}", sym.addr))
                                    .font(egui::FontId::monospace(theme.font_small))
                                    .color(theme.disasm_address),
                            );
                            ui.label(
                                egui::RichText::new(format!("{:#x}", sym.size))
                                    .font(egui::FontId::monospace(theme.font_small))
                                    .color(theme.text_muted),
                            );
                            ui.label(
                                egui::RichText::new(&sym.bind)
                                    .font(egui::FontId::monospace(theme.font_small))
                                    .color(theme.text_secondary),
                            );
                            ui.label(
                                egui::RichText::new(&sym.sym_type)
                                    .font(egui::FontId::monospace(theme.font_small))
                                    .color(theme.text_secondary),
                            );
                            ui.label(
                                egui::RichText::new(&sym.name)
                                    .font(egui::FontId::monospace(theme.font_small))
                                    .color(theme.text_primary),
                            );
                            ui.end_row();
                        }
                    });
                });
            });

            ui.add_space(8.0);

            // ── Disassembly ────────────────────────────────────────────────
            ui.label(
                egui::RichText::new(if info.is_dart_snapshot { "Disassembly (Dart AOT)" } else { "Disassembly" })
                    .strong()
                    .size(theme.font_heading)
                    .color(theme.text_accent),
            );
            if instructions.is_empty() {
                ui.label(
                    egui::RichText::new("No instructions found. No .text or .rodata code section was detected in this binary.")
                        .color(theme.text_muted)
                        .italics()
                        .size(theme.font_small),
                );
            } else {
                ui.add_space(2.0);
                egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                    let font = egui::FontId::monospace(12.0);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for insn in instructions {
                        let formatted = crate::native::disasm::Disassembler::format_instruction(insn);
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!("{:#010x}:", insn.address))
                                    .font(font.clone()).color(theme.disasm_address),
                            );
                            let hex = insn.bytes.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ");
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(format!("{:12}", hex))
                                    .font(font.clone()).color(theme.disasm_bytes),
                            );
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(&insn.mnemonic)
                                    .font(font.clone()).color(theme.disasm_mnemonic),
                            );
                            ui.label(
                                egui::RichText::new(&insn.operands)
                                    .font(font.clone()).color(theme.disasm_operand),
                            )
                            .on_hover_text(&formatted);
                        });
                    }
                });
            }
        });

        action
    }
}
