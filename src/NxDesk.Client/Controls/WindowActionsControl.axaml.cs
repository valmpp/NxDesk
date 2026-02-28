using Avalonia;
using Avalonia.Controls;
using Avalonia.Interactivity;
using Avalonia.Markup.Xaml;

namespace NxDesk.Client.Controls;

public partial class WindowActionsControl : UserControl
{
    public WindowActionsControl()
    {
        InitializeComponent();
    }

    private void MainButton_Click(object? sender, RoutedEventArgs e)
    {
        var panel = this.FindControl<Border>("ActionsPanel");
        if (panel != null)
        {
            panel.IsVisible = true;
        }
    }

    private void Minimize_Click(object? sender, RoutedEventArgs e)
    {
        var window = VisualRoot as Window;
        if (window != null) window.WindowState = WindowState.Minimized;
        HidePanel();
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
        HidePanel();
    }

    private void Close_Click(object? sender, RoutedEventArgs e)
    {
        var window = VisualRoot as Window;
        window?.Close();
    }

    private void HidePanel()
    {
        var panel = this.FindControl<Border>("ActionsPanel");
        if (panel != null)
        {
            panel.IsVisible = false;
        }
    }
}
