
import { SchemaField } from '../types';

export interface ValidationError {
  field: string;
  message: string;
}

export const validateRecord = (data: any, schema: SchemaField[]): ValidationError[] => {
  const errors: ValidationError[] = [];

  schema.forEach(field => {
    const value = data[field.name];

    // Required check
    if (field.required && (value === undefined || value === null || value === '')) {
      if (field.type === 'bool' && value === false) return; // False is valid for bool
      errors.push({ field: field.name, message: 'This field is required' });
      return;
    }

    if (value === undefined || value === null || value === '') return;

    // Type checks
    switch (field.type) {
      case 'email':
        const emailRegex = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
        if (!emailRegex.test(String(value))) {
          errors.push({ field: field.name, message: 'Invalid email address' });
        }
        break;
      case 'url':
        try {
          new URL(String(value));
        } catch {
          errors.push({ field: field.name, message: 'Invalid URL' });
        }
        break;
      case 'number':
        const num = Number(value);
        if (isNaN(num)) {
          errors.push({ field: field.name, message: 'Must be a number' });
        } else {
            if (field.min !== undefined && field.min !== null && num < field.min) {
                errors.push({ field: field.name, message: `Minimum value is ${field.min}` });
            }
            if (field.max !== undefined && field.max !== null && num > field.max) {
                errors.push({ field: field.name, message: `Maximum value is ${field.max}` });
            }
        }
        break;
      case 'json':
        if (typeof value === 'string') {
            try {
                JSON.parse(value);
            } catch {
                errors.push({ field: field.name, message: 'Invalid JSON string' });
            }
        }
        break;
    }
  });

  return errors;
};
