import React from 'react';
import { MapPin } from 'lucide-react';
import { Input } from './FormPrimitives';
import { FieldLabel } from './FieldLabel';

interface GeoPoint {
  lat: number;
  lng: number;
}

interface GeoPointInputProps {
  label: string;
  value?: GeoPoint;
  onChange: (value: GeoPoint) => void;
  required?: boolean;
  error?: string;
  className?: string;
}

export const GeoPointInput = React.forwardRef<HTMLInputElement, GeoPointInputProps>(({ label, value = { lat: 0, lng: 0 }, onChange, required, error, className }, ref) => {
  const handleChange = (key: 'lat' | 'lng', val: string) => {
    onChange({ ...value, [key]: parseFloat(val) || 0 });
  };

  return (
    <div className={`space-y-2 ${className}`}>
      <FieldLabel required={required} icon={<MapPin className="h-4 w-4" />}>{label}</FieldLabel>
      <div className="grid grid-cols-2 gap-4">
        <Input
          type="number"
          placeholder="Latitude"
          value={value.lat}
          onChange={(e) => handleChange('lat', e.target.value)}
          ref={ref}
        />
        <Input
          type="number"
          placeholder="Longitude"
          value={value.lng}
          onChange={(e) => handleChange('lng', e.target.value)}
        />
      </div>
       {error && <span className="text-xs text-destructive">{error}</span>}
    </div>
  );
});
GeoPointInput.displayName = 'GeoPointInput';
