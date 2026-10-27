using Avalonia.Controls;
using Avalonia.Input;

namespace NxDesk.Client.Views;

/// <summary>
/// Fullscreen overlay window that blocks all local user input.
/// Created by the Host when a remote admin sends a lock_screen signal.
/// </summary>
public partial class ScreenLockOverlay : Window
{
    public ScreenLockOverlay()
    {
        InitializeComponent();

        // Prevent the user from closing the overlay via Alt+F4 or window close
        Closing += (_, e) =>
        {
            if (!_allowClose)
                e.Cancel = true;
        };

        // Capture all key events to prevent ALT+TAB etc. from leaking
        KeyDown += (_, e) => e.Handled = true;
    }

    private bool _allowClose = false;

    /// <summary>
    /// Programmatically close the overlay (only callable by the ViewModel).
    /// </summary>
    public void ForceClose()
    {
        _allowClose = true;
        Close();
    }
}
