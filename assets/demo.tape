# wisu demo script for vhs

# 1. Setup the output and terminal properties
Output "assets/wisu-demo.gif"
Set Width 900
Set Height 557
Set FontSize 18
Set TypingSpeed 65ms
Set Theme "Catppuccin Frappe"
Set FontFamily "SpaceMono Nerd Font"

# 2. Show the classic view with icons
Type "wisu ../wisu_test_dir -g -L=2 --icons"
Sleep 1s
Enter
Sleep 5s
Ctrl+L

# 3. Show the classic view with size and permissions
Type "wisu ../wisu_test_dir -Ggps -L=2 --icons"
Sleep 1s
Enter
Sleep 5s
Ctrl+L

# 4. Show the interactive mode, pre-expanded
Type "wisu i ../wisu_test_dir -Ggps --icons --expand-level=2"
Sleep 1s
Enter
Sleep 2s

# 5. Navigate and interact with the TUI
Down 6
Sleep 1s
Up 5
Sleep 1s
Enter
Sleep 2s
Enter
Sleep 1s
Down 3
Sleep 2s
Enter
Sleep 5s
Type ":x"
Enter
Sleep 2s
