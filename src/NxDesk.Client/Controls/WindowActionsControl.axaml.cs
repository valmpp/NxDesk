using Avalonia;
using Avalonia.Controls;
using Avalonia.Interactivity;

namespace NxDesk.Client.Controls;

public partial class WindowActionsControl : UserControl
{
    public WindowActionsControl()
    {
        InitializeComponent();
    }

    private void Minimize_Click(object? sender, RoutedEventArgs e)
    {
        var window = VisualRoot as Window;
        if (window != null) window.WindowState = WindowState.Minimized;
    }

    private void Maximize_Click(object? sender, RoutedEventArgs e)
    {
        var window = VisualRoot as Window;
        if (window != null)
        {
            window.WindowState = window.WindowState == WindowState.Maximized 
                ? WindowState.Normal 
                : WindowState.Maximized;
        }
    }

    private void Close_Click(object? sender, RoutedEventArgs e)
    {
        var window = VisualRoot as Window;
        window?.Close();
    }
}
