using Avalonia.Data.Converters;
using Avalonia.Media;
using NxDesk.Core.Models;
using System;
using System.Globalization;

namespace NxDesk.Client.Controls;

public class PeerStatusToBrushConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is PeerStatus status)
        {
            return status switch
            {
                PeerStatus.Connected => Brushes.Green,    // Conectado
                PeerStatus.Background => Brushes.Red,     // Segundo plano
                PeerStatus.Offline => Brushes.Gray,       // No prendida / Desconectado
                _ => Brushes.Gray
            };
        }
        return Brushes.Gray;
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        throw new NotImplementedException();
    }
}
