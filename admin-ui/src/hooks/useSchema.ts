
import { useCallback } from 'react';
import { SchemaField } from '../types';
import { FIELD_TYPES_CONFIG } from '../config/field-types.config';

export function useSchema(schema: SchemaField[]) {
  
  const getDefaultValues = useCallback(() => {
    const defaults: Record<string, any> = {};
    schema.forEach(field => {
      switch(field.type) {
        case 'bool': defaults[field.name] = false; break;
        case 'number': defaults[field.name] = 0; break;
        case 'json': defaults[field.name] = '{}'; break;
        default: defaults[field.name] = '';
      }
    });
    return defaults;
  }, [schema]);

  const getFieldConfig = useCallback((type: string) => {
    return FIELD_TYPES_CONFIG[type as keyof typeof FIELD_TYPES_CONFIG];
  }, []);

  return {
    getDefaultValues,
    getFieldConfig
  };
}
