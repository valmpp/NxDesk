using Avalonia.Data.Converters;
using System;
using System.Globalization;

namespace NxDesk.Client.Controls;

public class EqualsIntConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is int val && int.TryParse(parameter?.ToString(), out int param))
        {
            return val == param;
        }
        return false;
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        throw new NotImplementedException();
    }
}

public class NotEqualsIntConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is int val && int.TryParse(parameter?.ToString(), out int param))
        {
            return val != param;
        }
        return true;
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        throw new NotImplementedException();
    }
}

public class BoolToControlTextConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        return value is true ? "Desactivar Control Remoto" : "Activar Control Remoto (Solo Visualización)";
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        throw new NotImplementedException();
    }
}

public class RemoteControlBrushConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        // Blue when active, muted gray when inactive
        return value is true ? Avalonia.Media.Brush.Parse("#007ACC") : Avalonia.Media.Brush.Parse("#888888");
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        throw new NotImplementedException();
    }
}
public class ScreenSelectionConverter : IMultiValueConverter
{
    public object? Convert(System.Collections.Generic.IList<object?> values, Type targetType, object? parameter, System.Globalization.CultureInfo culture)
    {
        if (values.Count >= 2 && values[0] is NxDesk.Application.DTOs.ScreenInfoDto item && values[1] is NxDesk.Application.DTOs.ScreenInfoDto selected)
        {
            return item.Index == selected.Index;
        }
        return false;
    }

    public object? ConvertBack(object? value, Type[] targetTypes, object? parameter, System.Globalization.CultureInfo culture)
    {
        throw new NotImplementedException();
    }
}
public class NullToBoolConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        return value == null;
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        throw new NotImplementedException();
    }
}

public class InverseBoolConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        return value is bool b ? !b : value;
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        return value is bool b ? !b : value;
    }
}

public class BoolToGridLengthConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is bool isExpanded)
        {
            // If expanded, use the parameter as the length (e.g. "2*" or "*")
            // If collapsed, use "Auto"
            return isExpanded ? Avalonia.Controls.GridLength.Parse(parameter?.ToString() ?? "*") : Avalonia.Controls.GridLength.Auto;
        }
        return Avalonia.Controls.GridLength.Auto;
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        throw new NotImplementedException();
    }
}

public class BoolToPathDataConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is bool b && parameter is string param)
        {
            var parts = param.Split('|');
            if (parts.Length == 2)
            {
                return b ? parts[0] : parts[1];
            }
        }
        return null;
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        throw new NotImplementedException();
    }
}

public class BoolToDoubleConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is bool b)
        {
            var parts = parameter?.ToString()?.Split('|');
            if (parts?.Length == 2)
            {
                double val1 = parts[0] == "NaN" ? double.NaN : double.Parse(parts[0], CultureInfo.InvariantCulture);
                double val2 = parts[1] == "NaN" ? double.NaN : double.Parse(parts[1], CultureInfo.InvariantCulture);
                return b ? val1 : val2;
            }
        }
        return double.NaN;
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) => throw new NotImplementedException();
}

public class BoolToThicknessConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is bool b && parameter is string param)
        {
            var parts = param.Split('|');
            if (parts.Length == 2)
            {
                return b ? Avalonia.Thickness.Parse(parts[0]) : Avalonia.Thickness.Parse(parts[1]);
            }
        }
        return new Avalonia.Thickness(0);
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) => throw new NotImplementedException();
}

public class NotificationTypeToBrushConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        string type = value?.ToString()?.ToLowerInvariant() ?? "";
        return type switch
        {
            "error" => Avalonia.Media.Brush.Parse("#D13438"),
            "success" => Avalonia.Media.Brush.Parse("#00CC66"),
            "warning" => Avalonia.Media.Brush.Parse("#FFD700"),
            _ => Avalonia.Media.Brush.Parse("#007ACC") // Info / Default
        };
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) => throw new NotImplementedException();
}

