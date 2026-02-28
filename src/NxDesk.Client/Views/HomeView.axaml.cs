using Avalonia.Controls;
using Avalonia.Interactivity;

namespace NxDesk.Client.Views;

public partial class HomeView : UserControl
{
    public HomeView()
    {
        InitializeComponent();
    }

    private async void CopyId_Click(object? sender, RoutedEventArgs e)
    {
        var window = VisualRoot as Window;
        var myId = (DataContext as ViewModels.MainWindowViewModel)?.MyId;
        
        if (window?.Clipboard != null && !string.IsNullOrEmpty(myId))
        {
            await window.Clipboard.SetTextAsync(myId);
        }
    }
}
