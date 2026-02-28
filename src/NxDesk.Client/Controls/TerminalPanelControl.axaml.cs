using Avalonia.Controls;
using Avalonia.Input;
using NxDesk.Client.ViewModels;

namespace NxDesk.Client.Controls;

public partial class TerminalPanelControl : UserControl
{
    public TerminalPanelControl()
    {
        InitializeComponent();
        DataContextChanged += OnDataContextChanged;
    }

    private void OnDataContextChanged(object? sender, System.EventArgs e)
    {
        if (DataContext is MainWindowViewModel vm)
        {
            vm.OnTerminalOutputReceived += ScrollToBottom;
        }
    }

    private void ScrollToBottom()
    {
        Avalonia.Threading.Dispatcher.UIThread.Post(() =>
        {
            TerminalScrollViewer?.ScrollToEnd();
        });
    }

    private void OnInputKeyDown(object? sender, KeyEventArgs e)
    {
        if (e.Key == Key.Enter && DataContext is MainWindowViewModel vm)
        {
            vm.SendTerminalInputCommand.Execute(null);
            e.Handled = true;
        }
    }
}
