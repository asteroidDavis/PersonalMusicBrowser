/// <reference path="../pb_data/types.d.ts" />

migrate((app) => {
  const shares = new Collection({
    type: "base",
    name: "shares",
    listRule: '@request.auth.id != "" && (user_id = @request.auth.id || created_by = @request.auth.id)',
    viewRule: '@request.auth.id != "" && (user_id = @request.auth.id || created_by = @request.auth.id)',
    createRule: '@request.auth.id != "" && created_by = @request.auth.id',
    updateRule: '@request.auth.id != "" && created_by = @request.auth.id',
    deleteRule: '@request.auth.id != "" && created_by = @request.auth.id',
    fields: [
      { name: "user_id", type: "text", required: true },
      { name: "resource_type", type: "text", required: true },
      { name: "resource_id", type: "text", required: true },
      { name: "access_level", type: "select", required: true, maxSelect: 1, values: ["viewer", "editor", "admin"] },
      { name: "created_by", type: "text", required: true },
    ],
    indexes: [
      "CREATE INDEX idx_shares_user_resource ON shares (user_id, resource_type, resource_id)",
      "CREATE INDEX idx_shares_resource ON shares (resource_type, resource_id)",
      "CREATE INDEX idx_shares_created_by ON shares (created_by)",
    ],
  });

  const groups = new Collection({
    type: "base",
    name: "groups",
    listRule: '@request.auth.id != ""',
    viewRule: '@request.auth.id != ""',
    createRule: '@request.auth.id != "" && owner_id = @request.auth.id',
    updateRule: '@request.auth.id != "" && owner_id = @request.auth.id',
    deleteRule: '@request.auth.id != "" && owner_id = @request.auth.id',
    fields: [
      { name: "name", type: "text", required: true },
      { name: "description", type: "text", required: false },
      { name: "owner_id", type: "text", required: true },
    ],
  });

  const groupMemberships = new Collection({
    type: "base",
    name: "group_memberships",
    listRule: '@request.auth.id != "" && user_id = @request.auth.id',
    viewRule: '@request.auth.id != "" && user_id = @request.auth.id',
    createRule: '@request.auth.id != ""',
    updateRule: '@request.auth.id != ""',
    deleteRule: '@request.auth.id != ""',
    fields: [
      { name: "group_id", type: "text", required: true },
      { name: "user_id", type: "text", required: true },
      { name: "role", type: "select", required: true, maxSelect: 1, values: ["owner", "admin", "member", "viewer"] },
    ],
  });

  const groupShares = new Collection({
    type: "base",
    name: "group_shares",
    listRule: '@request.auth.id != ""',
    viewRule: '@request.auth.id != ""',
    createRule: '@request.auth.id != "" && created_by = @request.auth.id',
    updateRule: '@request.auth.id != "" && created_by = @request.auth.id',
    deleteRule: '@request.auth.id != "" && created_by = @request.auth.id',
    fields: [
      { name: "group_id", type: "text", required: true },
      { name: "resource_type", type: "text", required: true },
      { name: "resource_id", type: "text", required: true },
      { name: "access_level", type: "select", required: true, maxSelect: 1, values: ["viewer", "editor", "admin"] },
      { name: "created_by", type: "text", required: true },
    ],
  });

  app.save(shares);
  app.save(groups);
  app.save(groupMemberships);
  app.save(groupShares);
}, (app) => {
  try { app.delete(app.findCollectionByNameOrId("group_shares")); } catch {}
  try { app.delete(app.findCollectionByNameOrId("group_memberships")); } catch {}
  try { app.delete(app.findCollectionByNameOrId("groups")); } catch {}
  try { app.delete(app.findCollectionByNameOrId("shares")); } catch {}
});
