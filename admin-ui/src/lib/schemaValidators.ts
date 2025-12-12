import { SchemaField } from '../types';

export interface ValidationError {
  field: string;
  message: string;
}

export const validateRecord = (data: any, schema: SchemaField[]): ValidationError[] => {
  const errors: ValidationError[] = [];

  schema.forEach(field => {
    const value = data[field.name];

    // 1. Required Check
    if (field.required && (value === undefined || value === null || value === '')) {
      if (field.type === 'bool' && value === false) return; 
      if (field.type === 'number' && value === 0) return;
      errors.push({ field: field.name, message: 'This field is required' });
      return;
    }

    if (value === undefined || value === null || value === '') return;

    // 2. Type & Pattern Checks
    switch (field.type) {
      case 'email':
        if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(String(value))) {
          errors.push({ field: field.name, message: 'Invalid email address' });
        }
        break;
        
      case 'url':
        try { new URL(String(value)); } catch {
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
            try { JSON.parse(value); } catch {
                errors.push({ field: field.name, message: 'Invalid JSON string' });
            }
        }
        break;
        
      case 'vector':
        let vec: any[];
        if (Array.isArray(value)) vec = value;
        else if (typeof value === 'string') {
            try { vec = JSON.parse(value); } catch { 
                errors.push({ field: field.name, message: 'Invalid vector format' }); 
                return;
            }
        } else {
            errors.push({ field: field.name, message: 'Expected array for vector' });
            return;
        }
        
        if (!Array.isArray(vec)) {
             errors.push({ field: field.name, message: 'Expected array' });
        } else {
             if (field.dimension && vec.length !== field.dimension) {
                 errors.push({ field: field.name, message: `Vector dimension mismatch. Expected ${field.dimension}, got ${vec.length}` });
             }
             if (vec.some(n => isNaN(Number(n)))) {
                 errors.push({ field: field.name, message: 'Vector must contain numbers' });
             }
        }
        break;
        
      case 'string':
      case 'text':
        const strLen = String(value).length;
        if (field.minLength && strLen < field.minLength) {
            errors.push({ field: field.name, message: `Too short. Min ${field.minLength} chars.` });
        }
        if (field.maxLength && strLen > field.maxLength) {
            errors.push({ field: field.name, message: `Too long. Max ${field.maxLength} chars.` });
        }
        if (field.pattern) {
            try {
                const re = new RegExp(field.pattern);
                if (!re.test(String(value))) {
                    errors.push({ field: field.name, message: 'Format does not match required pattern.' });
                }
            } catch (e) { /* Ignore invalid regex in definition */ }
        }
        break;
    }
  });

  return errors;
};