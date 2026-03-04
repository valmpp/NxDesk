using System;
using Microsoft.Extensions.DependencyInjection;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using NxDesk.Client.ViewModels;

namespace NxDesk.Client.Views;

public partial class MainWindow : Window
{
    private readonly IServiceProvider _serviceProvider;
    public bool IsExiting { get; set; } = false;

    public MainWindow(IServiceProvider serviceProvider)
    {
        _serviceProvider = serviceProvider;
        InitializeComponent();
    }

    protected override void OnClosing(WindowClosingEventArgs e)
    {
        if (!IsExiting)
        {
            e.Cancel = true;
            this.Hide();
        }
        else
        {
            base.OnClosing(e);
        }
    }

    protected override void OnOpened(EventArgs e)
    {
        base.OnOpened(e);
        if (DataContext is MainWindowViewModel vm)
        {
            vm.InitializeFromArgs(Environment.GetCommandLineArgs());

            // Setup Pickers
            vm.RequestFilePicker = async () => 
            {
                var files = await StorageProvider.OpenFilePickerAsync(new Avalonia.Platform.Storage.FilePickerOpenOptions
                {
                    Title = "Seleccionar archivo para enviar",
                    AllowMultiple = false
                });
                return files.Count > 0 ? files[0].Path.LocalPath : null;
            };

            vm.RequestFolderPicker = async () => 
            {
                var folders = await StorageProvider.OpenFolderPickerAsync(new Avalonia.Platform.Storage.FolderPickerOpenOptions
                {
                    Title = "Seleccionar carpeta para enviar",
                    AllowMultiple = false
                });
                return folders.Count > 0 ? folders[0].Path.LocalPath : null;
            };
        }
    }

    private void OnPointerPressed(object? sender, PointerPressedEventArgs e)
    {
        if (e.GetCurrentPoint(this).Properties.IsLeftButtonPressed)
        {
            BeginMoveDrag(e);
        }
    }

    private void OnSettingsClick(object? sender, RoutedEventArgs e)
    {
        var settingsWin = new SettingsWindow
        {
            DataContext = _serviceProvider.GetRequiredService<SettingsViewModel>()
        };
        settingsWin.ShowDialog(this);
    }

    private void OnMinimizeClick(object sender, RoutedEventArgs e)
    {
        this.WindowState = WindowState.Minimized;
    }

    private void OnCloseClick(object sender, RoutedEventArgs e)
    {
        this.Close();
    }

    private void OnRoomIdKeyDown(object? sender, KeyEventArgs e)
    {
        if (e.Key == Key.Enter && DataContext is MainWindowViewModel vm)
        {
            vm.ConnectByIdCommand.Execute(null);
            e.Handled = true;
        }
    }
}