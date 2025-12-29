# Backlog

## Known Issues

### TextInput cursor positioning after window resize
- **Description**: Cursor doesn't properly set position when clicked on the text_input after window resize
- **Location**: `crates/blinc_layout/src/widgets/text_input.rs`
- **Likely cause**: Layout bounds stored for scroll calculation become stale after resize, affecting click-to-cursor position mapping
