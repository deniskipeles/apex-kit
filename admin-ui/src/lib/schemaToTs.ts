import { Collection, SchemaField } from '../types';

const mapType = (type: string): string => {
  switch (type) {
    case 'number':
      return 'number';
    case 'bool':
      return 'boolean';
    case 'json':
      return 'any';
    // Relation/Owner are usually stored as IDs (strings/numbers) in the DB record
    // unless expanded, but for basic DB ops, they are values.
    case 'relation':
    case 'owner':
      return 'string | number';
    default:
      return 'string'; // text, email, url, date, file, etc.
  }
};

export const generateTypeScriptDefs = (collections: Collection[]): string => {
  let typeDefs = `
    // --- Base Types ---
    interface BaseRecord {
        id: number;
        created_at?: string;
        updated_at?: string;
    }
    `;

  const collectionNames: string[] = [];

  // 1. Generate Interfaces for each Collection
  collections.forEach((col) => {
    const interfaceName =
      col.name.charAt(0).toUpperCase() + col.name.slice(1).replace(/[^a-zA-Z0-9]/g, '');
    collectionNames.push(`"${col.name}": ${interfaceName}`);

    typeDefs += `
    interface ${interfaceName} extends BaseRecord {
`;
    col.schema.forEach((field) => {
      const tsType = mapType(field.type);
      const optional = !field.required ? '?' : '';
      typeDefs += `        ${field.name}${optional}: ${tsType};\n`;
    });
    typeDefs += `    }\n`;
  });

  // 2. Create the Collection Map
  typeDefs += `
    interface CollectionMap {
        ${collectionNames.join(';\n        ')}
    }
    `;

  // 3. Define Global Objects with Generics
  typeDefs += `
    declare const $db: {
        /**
         * Find a single record by ID.
         */
        find_one<K extends keyof CollectionMap>(col: K, id: number | string): Promise<CollectionMap[K] | null>;
        find_one(col: string, id: number | string): Promise<any>;

        /**
         * Find records matching a filter.
         * @example $db.find('posts', { is_published: true })
         */
        find<K extends keyof CollectionMap>(col: K, filter?: Partial<CollectionMap[K]> | object): Promise<CollectionMap[K][]>;
        find(col: string, filter?: object): Promise<any[]>;

        /**
         * Insert a new record. Returns the new ID.
         */
        insert<K extends keyof CollectionMap>(col: K, data: Partial<CollectionMap[K]>): Promise<number>;
        insert(col: string, data: object): Promise<number>;

        /**
         * Update a record by ID.
         */
        update<K extends keyof CollectionMap>(col: K, id: number | string, data: Partial<CollectionMap[K]>): Promise<CollectionMap[K]>;
        update(col: string, id: number | string, data: object): Promise<any>;

        /**
         * Delete a record by ID.
         */
        delete(col: string, id: number | string): Promise<boolean>;
    };

    declare const $http: {
        get(url: string): Promise<string>;
        post(url: string, body: object): Promise<string>;
    };

    declare const $util: {
        uuid(): string;
    };

    declare const $ai: {
        embed(text: string, provider?: string): Promise<number[]>;
    };

    declare const $env: {
        get(key: string): Promise<string>;
    };

    declare function log(msg: any): void;
    `;

  return typeDefs;
};
