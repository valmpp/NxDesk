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
        //BLOQUEAR Control + v, control y control X
        if ((e.KeyModifiers & KeyModifiers.Control)  != 0 &&
        (e.Key == Key.V ||
         e.Key == Key.C  ||
         e.Key == Key.X))
        {
            e.Handled = true;
            return;
        } 

         //Comprueba al presionar Enteder
        if (e.Key == Key.Enter && DataContext is MainWindowViewModel vm)
        {
            vm.ConnectByIdCommand.Execute(null);
            e.Handled = true;
            return;
        }
        //Te permitira mantener las teclas de edicion y navegacion

        if(
            e.Key == Key.Back || 
            e.Key == Key.Delete || 
            e.Key == Key.Tab || 
            e.Key == Key.Left ||
            e.Key == Key.Right ||
            e.Key == Key.Home ||
            e.Key == Key.End)
            {
                return;
            }
            //Comprobar si la tecla corresponde numeros de 0 al 9

            bool esNumero = e.Key == Key.OemPeriod ||
                             e.Key == Key.Decimal;

            for(int i = 0; i <= 9; i++ )
            {
                if(e.Key == Key.D0 + i || e.Key == Key.NumPad0 + i)
                {
                    esNumero = true;
                    break;
                }
            }
            if(!esNumero)
                e.Handled = true;
            return;

    }

    private void OnRoomIdTextInput(object? sender, TextInputEventArgs e)
      {
        if (string.IsNullOrEmpty(e.Text))
        return;

        foreach (char c in e.Text)
        {
           if (!char.IsDigit(c) && c != '.')
           {
            e.Handled = true;
            return;
           }
        }
      }

    private void OnRoomIdCopying(
       object? sender, RoutedEventArgs e)
       {
         e.Handled = true;
       }

    private void OnRoomIdPasting(
        object? sender, RoutedEventArgs e) 
       {
         e.Handled = true;
       }

    private void OnRoomIdCutting(
        object? sender, RoutedEventArgs e)
       {
         e.Handled = true;
       }
   
}