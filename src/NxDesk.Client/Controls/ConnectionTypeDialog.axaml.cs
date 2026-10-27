using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Markup.Xaml;
using NxDesk.Client.ViewModels;

namespace NxDesk.Client.Controls;

public partial class ConnectionTypeDialog : UserControl
{
    public ConnectionTypeDialog()
    {
        InitializeComponent();
    }

    private void ViewOnly_Click(object? sender, PointerPressedEventArgs e)
    {
        if (DataContext is MainWindowViewModel vm)
        {
            vm.IsControlModeSelected = false;
        }
    }

    private void Control_Click(object? sender, PointerPressedEventArgs e)
    {
        if (DataContext is MainWindowViewModel vm)
        {
            vm.IsControlModeSelected = true;
        }
    }
}
