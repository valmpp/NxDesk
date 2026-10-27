using Avalonia.Controls;
using System.Threading.Tasks;
using NxDesk.Client.Views;

namespace NxDesk.Client.Services;

public interface IDialogService
{
    Task ShowSettingsAsync(Window owner);
}

public class DialogService : IDialogService
{
    public async Task ShowSettingsAsync(Window owner)
    {
        var dialog = new SettingsWindow();
        await dialog.ShowDialog(owner);
    }
}
