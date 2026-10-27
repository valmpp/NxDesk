using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;

namespace NxDesk.Client.Views;

public partial class SettingsWindow : Window
{
    public SettingsWindow()
    {
        InitializeComponent(); 
    }

    private void OnCloseClick(object? sender, RoutedEventArgs e)
    {
        Close();
    }

    private void OnPointerPressed(object? sender, PointerPressedEventArgs e)
    {
        if (e.GetCurrentPoint(this).Properties.IsLeftButtonPressed) 
        {
            BeginMoveDrag(e);
        }
    }
}
