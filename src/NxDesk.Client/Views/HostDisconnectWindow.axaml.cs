using System;
using Avalonia;
using Avalonia.Controls;

namespace NxDesk.Client.Views;

public partial class HostDisconnectWindow : Window
{
    private bool _allowClose = false;

    public HostDisconnectWindow()
    {
        InitializeComponent();

        // Support dragging the window from the content area
        PointerPressed += (sender, e) =>
        {
            if (e.GetCurrentPoint(this).Properties.IsLeftButtonPressed)
            {
                BeginMoveDrag(e);
            }
        };
    }

    /// <summary>
    /// Call this method to programmatically close the window (e.g. when the session ends).
    /// The regular Close() is intercepted to prevent the user from closing directly.
    /// </summary>
    public void ForceClose()
    {
        _allowClose = true;
        Close();
    }

    public void Minimize_Click(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
    {
        WindowState = WindowState.Minimized;
    }

    protected override void OnClosing(WindowClosingEventArgs e)
    {
        // Prevent the user from closing this window manually (e.g. via Alt+F4)
        // Only allow programmatic close via ForceClose()
        if (!_allowClose)
        {
            e.Cancel = true;
            // Minimize instead of closing
            WindowState = WindowState.Minimized;
            return;
        }
        base.OnClosing(e);
    }
}
