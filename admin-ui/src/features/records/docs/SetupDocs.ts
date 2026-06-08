export const getSetupDocs = (apiBaseUrl: string, pathname: string) => {
  let initCode = `import { ApexKit } from '@apexkit/sdk';\n\nconst apex = new ApexKit('${apiBaseUrl}');`;

  if (pathname.includes('/tenant/')) {
    const tenantId = pathname.split('/tenant/')[1].split('/')[0];
    initCode += `\n\n// Target Specific Tenant\nconst client = apex.tenant('${tenantId}');`;
  } else if (pathname.includes('/sandbox/')) {
    const sandboxId = pathname.split('/sandbox/')[1].split('/')[0];
    initCode += `\n\n// Target Sandbox Session\nconst client = apex.sandbox('${sandboxId}');`;
  } else {
    initCode += `\n\nconst client = apex;`;
  }

  return { initCode };
};
