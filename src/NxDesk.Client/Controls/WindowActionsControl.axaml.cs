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

    private void MainButton_Click(object? sender, RoutedEventArgs e)
    {
        var wrapper = this.FindControl<StackPanel>("ControlWrapper");
        if (wrapper != null && !wrapper.Classes.Contains("expanded"))
        {
            wrapper.Classes.Add("expanded");
        }
    }

    private void ControlWrapper_PointerExited(object? sender, Avalonia.Input.PointerEventArgs e)
    {
        var wrapper = this.FindControl<StackPanel>("ControlWrapper");
        if (wrapper != null)
        {
            wrapper.Classes.Remove("expanded");
        }
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
