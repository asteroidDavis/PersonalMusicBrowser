/// <reference path="../pb_data/types.d.ts" />

// This migration seeds two known test users so the music_browser
// pocketbase_client integration tests (tests/pocketbase_client_integration_tests.rs)
// have real, authenticatable accounts to exercise the shares/groups ACL
// collections against.
//
// It intentionally does nothing unless PB_TEST_SEED=true is set in the
// environment of the `pocketbase` process, so it is a no-op against real
// dev/prod databases. CI and the local integration-test runner
// (music_browser/scripts/run-pocketbase-integration-tests.sh) both set this
// variable before running `pocketbase migrate up` / `pocketbase serve`.
migrate((app) => {
  if (process.env.PB_TEST_SEED !== "true") {
    return;
  }

  const users = app.findCollectionByNameOrId("users");
  const testUsers = [
    { email: "acl-test-user-1@example.com", password: "AclTestUser1Password!" },
    { email: "acl-test-user-2@example.com", password: "AclTestUser2Password!" },
  ];

  for (const testUser of testUsers) {
    try {
      app.findAuthRecordByEmail("users", testUser.email);
      continue; // already seeded
    } catch {
      // not found, fall through and create it
    }

    const record = new Record(users);
    record.setEmail(testUser.email);
    record.setPassword(testUser.password);
    record.setVerified(true);
    app.save(record);
  }
}, (app) => {
  if (process.env.PB_TEST_SEED !== "true") {
    return;
  }

  const testEmails = ["acl-test-user-1@example.com", "acl-test-user-2@example.com"];
  for (const email of testEmails) {
    try {
      const record = app.findAuthRecordByEmail("users", email);
      app.delete(record);
    } catch {
      // already gone
    }
  }
});
