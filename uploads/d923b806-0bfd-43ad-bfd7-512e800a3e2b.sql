BEGIN TRANSACTION;
CREATE TABLE IF NOT EXISTS "students" (
	"avatar"	TEXT NOT NULL DEFAULT '',
	"created"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"email"	TEXT NOT NULL DEFAULT '',
	"emailVisibility"	BOOLEAN NOT NULL DEFAULT FALSE,
	"id"	TEXT NOT NULL DEFAULT ('r' || lower(hex(randomblob(7)))),
	"lastResetSentAt"	TEXT NOT NULL DEFAULT '',
	"lastVerificationSentAt"	TEXT NOT NULL DEFAULT '',
	"name"	TEXT NOT NULL DEFAULT '',
	"passwordHash"	TEXT NOT NULL,
	"tokenKey"	TEXT NOT NULL,
	"updated"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"username"	TEXT NOT NULL,
	"verified"	BOOLEAN NOT NULL DEFAULT FALSE,
	PRIMARY KEY("id")
);
CREATE TABLE IF NOT EXISTS "schools" (
	"created"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"description"	TEXT NOT NULL DEFAULT '',
	"email"	TEXT NOT NULL DEFAULT '',
	"id"	TEXT NOT NULL DEFAULT ('r' || lower(hex(randomblob(7)))),
	"images"	JSON NOT NULL DEFAULT '[]',
	"logo"	TEXT NOT NULL DEFAULT '',
	"logo_name"	TEXT NOT NULL DEFAULT '',
	"more_details"	JSON DEFAULT NULL,
	"name"	TEXT NOT NULL DEFAULT '',
	"number_of_staff"	NUMERIC NOT NULL DEFAULT 0,
	"number_of_students"	NUMERIC NOT NULL DEFAULT 0,
	"school_level"	TEXT NOT NULL DEFAULT '',
	"updated"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	PRIMARY KEY("id")
);
CREATE TABLE IF NOT EXISTS "books_collections_borrowed" (
	"book_collection_id"	TEXT NOT NULL DEFAULT '',
	"check_in_comment"	TEXT NOT NULL DEFAULT '',
	"created"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"expected_to_returned_on"	TEXT NOT NULL DEFAULT '',
	"id"	TEXT NOT NULL DEFAULT ('r' || lower(hex(randomblob(7)))),
	"returned"	BOOLEAN NOT NULL DEFAULT FALSE,
	"student_id"	TEXT NOT NULL DEFAULT '',
	"updated"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"check_in_on"	TEXT NOT NULL DEFAULT '',
	PRIMARY KEY("id")
);
CREATE TABLE IF NOT EXISTS "librarians" (
	"active"	BOOLEAN NOT NULL DEFAULT FALSE,
	"created"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"id"	TEXT NOT NULL DEFAULT ('r' || lower(hex(randomblob(7)))),
	"library_id"	TEXT NOT NULL DEFAULT '',
	"staff_id"	TEXT NOT NULL DEFAULT '',
	"updated"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	PRIMARY KEY("id")
);
CREATE TABLE IF NOT EXISTS "libraries" (
	"active"	BOOLEAN NOT NULL DEFAULT FALSE,
	"created"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"description"	TEXT NOT NULL DEFAULT '',
	"id"	TEXT NOT NULL DEFAULT ('r' || lower(hex(randomblob(7)))),
	"more_details"	JSON DEFAULT NULL,
	"name"	TEXT NOT NULL DEFAULT '',
	"number_of_books"	NUMERIC NOT NULL DEFAULT 0,
	"school_id"	TEXT NOT NULL DEFAULT '',
	"updated"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	PRIMARY KEY("id")
);
CREATE TABLE IF NOT EXISTS "books_collections" (
	"added_on"	TEXT NOT NULL DEFAULT '',
	"book_id"	TEXT NOT NULL DEFAULT '',
	"created"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"id"	TEXT NOT NULL DEFAULT ('r' || lower(hex(randomblob(7)))),
	"index"	TEXT NOT NULL DEFAULT '',
	"updated"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	PRIMARY KEY("id")
);
CREATE TABLE IF NOT EXISTS "books" (
	"covers"	JSON NOT NULL DEFAULT '[]',
	"created"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"description"	TEXT NOT NULL DEFAULT '',
	"id"	TEXT NOT NULL DEFAULT ('r' || lower(hex(randomblob(7)))),
	"library_id"	TEXT NOT NULL DEFAULT '',
	"maximum_days_to_borrow"	NUMERIC NOT NULL DEFAULT 0,
	"name"	TEXT NOT NULL DEFAULT '',
	"updated"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"class_level_id"	TEXT NOT NULL DEFAULT '',
	"subject_id"	TEXT NOT NULL DEFAULT '',
	PRIMARY KEY("id")
);
CREATE TABLE IF NOT EXISTS "staff" (
	"avatar"	TEXT NOT NULL DEFAULT '',
	"created"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"description"	TEXT NOT NULL DEFAULT '',
	"email"	TEXT NOT NULL DEFAULT '',
	"emailVisibility"	BOOLEAN NOT NULL DEFAULT FALSE,
	"id"	TEXT NOT NULL DEFAULT ('r' || lower(hex(randomblob(7)))),
	"lastResetSentAt"	TEXT NOT NULL DEFAULT '',
	"lastVerificationSentAt"	TEXT NOT NULL DEFAULT '',
	"name"	TEXT NOT NULL DEFAULT '',
	"other_roles"	JSON DEFAULT NULL,
	"passwordHash"	TEXT NOT NULL,
	"phone_number"	TEXT NOT NULL DEFAULT '',
	"role"	TEXT NOT NULL DEFAULT '',
	"tokenKey"	TEXT NOT NULL,
	"updated"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"username"	TEXT NOT NULL,
	"verified"	BOOLEAN NOT NULL DEFAULT FALSE,
	PRIMARY KEY("id")
);
CREATE TABLE IF NOT EXISTS "notes" (
	"created"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"document"	TEXT NOT NULL DEFAULT '',
	"id"	TEXT NOT NULL DEFAULT ('r' || lower(hex(randomblob(7)))),
	"staff_id"	TEXT NOT NULL DEFAULT '',
	"subject"	TEXT NOT NULL DEFAULT '',
	"summary"	TEXT NOT NULL DEFAULT '',
	"title"	TEXT NOT NULL DEFAULT '',
	"updated"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	PRIMARY KEY("id")
);
CREATE TABLE IF NOT EXISTS "accountings" (
	"created"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"description"	TEXT NOT NULL DEFAULT '',
	"id"	TEXT NOT NULL DEFAULT ('r' || lower(hex(randomblob(7)))),
	"name"	TEXT NOT NULL DEFAULT '',
	"school_id"	TEXT NOT NULL DEFAULT '',
	"section"	TEXT NOT NULL DEFAULT '',
	"updated"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	PRIMARY KEY("id")
);
CREATE TABLE IF NOT EXISTS "accountants" (
	"accounting_id"	TEXT NOT NULL DEFAULT '',
	"active"	BOOLEAN NOT NULL DEFAULT FALSE,
	"created"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"id"	TEXT NOT NULL DEFAULT ('r' || lower(hex(randomblob(7)))),
	"staff_id"	TEXT NOT NULL DEFAULT '',
	"updated"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	PRIMARY KEY("id")
);
CREATE TABLE IF NOT EXISTS "questions" (
	"created"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"id"	TEXT NOT NULL DEFAULT ('r' || lower(hex(randomblob(7)))),
	"question"	JSON DEFAULT NULL,
	"school_id"	TEXT NOT NULL DEFAULT '',
	"staff_id"	TEXT NOT NULL DEFAULT '',
	"subject"	TEXT NOT NULL DEFAULT '',
	"updated"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"marks"	NUMERIC NOT NULL DEFAULT 0,
	PRIMARY KEY("id")
);
CREATE TABLE IF NOT EXISTS "question_student_answer" (
	"answer"	JSON DEFAULT NULL,
	"created"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"id"	TEXT NOT NULL DEFAULT ('r' || lower(hex(randomblob(7)))),
	"question_id"	TEXT NOT NULL DEFAULT '',
	"student_id"	TEXT NOT NULL DEFAULT '',
	"updated"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	PRIMARY KEY("id")
);
CREATE TABLE IF NOT EXISTS "class_levels" (
	"class"	NUMERIC NOT NULL DEFAULT 0,
	"created"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"description"	TEXT NOT NULL DEFAULT '',
	"id"	TEXT NOT NULL DEFAULT ('r' || lower(hex(randomblob(7)))),
	"name"	TEXT NOT NULL DEFAULT '',
	"updated"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	PRIMARY KEY("id")
);
CREATE TABLE IF NOT EXISTS "subjects" (
	"created"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"description"	TEXT NOT NULL DEFAULT '',
	"id"	TEXT NOT NULL DEFAULT ('r' || lower(hex(randomblob(7)))),
	"name"	TEXT NOT NULL DEFAULT '',
	"updated"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	PRIMARY KEY("id")
);
CREATE TABLE IF NOT EXISTS "subjects_per_classes" (
	"class_id"	TEXT NOT NULL DEFAULT '',
	"created"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"id"	TEXT NOT NULL DEFAULT ('r' || lower(hex(randomblob(7)))),
	"subject_id"	TEXT NOT NULL DEFAULT '',
	"updated"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"name"	TEXT NOT NULL DEFAULT '',
	PRIMARY KEY("id")
);
CREATE TABLE IF NOT EXISTS "classes" (
	"academic_year"	TEXT NOT NULL DEFAULT '',
	"created"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"id"	TEXT NOT NULL DEFAULT ('r' || lower(hex(randomblob(7)))),
	"class_level_id"	TEXT NOT NULL DEFAULT '',
	"school_id"	TEXT NOT NULL DEFAULT '',
	"updated"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"year"	TEXT NOT NULL DEFAULT '',
	"more_details"	JSON DEFAULT NULL,
	PRIMARY KEY("id")
);
CREATE TABLE IF NOT EXISTS "student_class_enrollment" (
	"class_id"	TEXT NOT NULL DEFAULT '',
	"created"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"id"	TEXT NOT NULL DEFAULT ('r' || lower(hex(randomblob(7)))),
	"student_id"	TEXT NOT NULL DEFAULT '',
	"staff_id"	TEXT NOT NULL DEFAULT '',
	"updated"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"subjects_ids"	JSON NOT NULL DEFAULT '[]',
	PRIMARY KEY("id")
);
CREATE TABLE IF NOT EXISTS "class_teachers" (
	"class_id"	TEXT NOT NULL DEFAULT '',
	"created"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"from_date"	TEXT NOT NULL DEFAULT '',
	"id"	TEXT NOT NULL DEFAULT ('r' || lower(hex(randomblob(7)))),
	"staff_id"	TEXT NOT NULL DEFAULT '',
	"updated"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	PRIMARY KEY("id")
);
CREATE TABLE IF NOT EXISTS "subjects_per_class_staff" (
	"active"	BOOLEAN NOT NULL DEFAULT FALSE,
	"created"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"from_date"	TEXT NOT NULL DEFAULT '',
	"id"	TEXT NOT NULL DEFAULT ('r' || lower(hex(randomblob(7)))),
	"staff_id"	TEXT NOT NULL DEFAULT '',
	"subjects_per_class_id"	TEXT NOT NULL DEFAULT '',
	"to_date"	TEXT NOT NULL DEFAULT '',
	"updated"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	PRIMARY KEY("id")
);
CREATE TABLE IF NOT EXISTS "class_attendances" (
	"class_id"	TEXT NOT NULL DEFAULT '',
	"created"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"date"	TEXT NOT NULL DEFAULT '',
	"excuse"	TEXT NOT NULL DEFAULT '',
	"id"	TEXT NOT NULL DEFAULT ('r' || lower(hex(randomblob(7)))),
	"present"	BOOLEAN NOT NULL DEFAULT FALSE,
	"staff_id"	TEXT NOT NULL DEFAULT '',
	"student_id"	TEXT NOT NULL DEFAULT '',
	"updated"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	PRIMARY KEY("id")
);
CREATE TABLE IF NOT EXISTS "tests" (
	"created"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"description"	TEXT NOT NULL DEFAULT '',
	"id"	TEXT NOT NULL DEFAULT ('r' || lower(hex(randomblob(7)))),
	"name"	TEXT NOT NULL DEFAULT '',
	"school_id"	TEXT NOT NULL DEFAULT '',
	"staff_id"	TEXT NOT NULL DEFAULT '',
	"updated"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	PRIMARY KEY("id")
);
CREATE TABLE IF NOT EXISTS "test_questions" (
	"created"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"id"	TEXT NOT NULL DEFAULT ('r' || lower(hex(randomblob(7)))),
	"marks"	NUMERIC NOT NULL DEFAULT 0,
	"question_id"	TEXT NOT NULL DEFAULT '',
	"test_id"	TEXT NOT NULL DEFAULT '',
	"updated"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	PRIMARY KEY("id")
);
CREATE TABLE IF NOT EXISTS "test_student_results" (
	"comment"	TEXT NOT NULL DEFAULT '',
	"created"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"id"	TEXT NOT NULL DEFAULT ('r' || lower(hex(randomblob(7)))),
	"marks"	NUMERIC NOT NULL DEFAULT 0,
	"student_id"	TEXT NOT NULL DEFAULT '',
	"test_id"	TEXT NOT NULL DEFAULT '',
	"updated"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	PRIMARY KEY("id")
);
CREATE TABLE IF NOT EXISTS "test_questions_student_answers" (
	"answer"	JSON DEFAULT NULL,
	"created"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	"id"	TEXT NOT NULL DEFAULT ('r' || lower(hex(randomblob(7)))),
	"student_id"	TEXT NOT NULL DEFAULT '',
	"test_question_id"	TEXT NOT NULL DEFAULT '',
	"updated"	TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%fZ')),
	PRIMARY KEY("id")
);
INSERT INTO "students" VALUES ('dream_shaper_v7_full_body_cyber_girl_with_a_ponytail_ariana_gra_1_4RbSDbFLpQ.jpg','2023-09-15 11:09:06.990Z','narydjin@gmail.com',0,'nithx4ol1hu6y75','','','Denis Kemboi','$2a$12$r2y60ku6khrdmOrTtZ3/I.Xaz4U8M9Ju9iGnf44tBxMGXMP.7ReNm','bU74b6k17QyWXa0PFAhbZ5YVjGdrSuZJsMXVFMwzZoaxc8kk1P','2023-09-15 12:09:04.760Z','students31132',0);
INSERT INTO "schools" VALUES ('2023-09-15 11:22:07.712Z','','narydbest@gmail.com','0i8719g7sqks9vn','[]','dream_shaper_v7_full_body_cyber_girl_with_a_ponytail_ariana_gra_0_1_qDX71vL1S1.jpg','simotwet','null','simotwet primary school',0,0,'primary','2023-09-15 11:22:07.712Z');
INSERT INTO "staff" VALUES ('dream_shaper_v7_full_body_cyber_girl_with_a_ponytail_ariana_gra_0_HwH49z8tVU.jpg','2023-09-15 12:18:52.844Z','About me','narydjin@gmail.com',0,'6icitq7qbvlqtkr','','','Denis Kemboi',NULL,'$2a$12$R.8DBqyCftxJYGQfa1kwCuERsUqNsUWK9se8DPIOlaA5xxxSPZD7.','+254 712345678','','dsNBnVz2RyjJ0HjxcKLgZdLam1po2KcZg82MriT5nj9kHDhtH2','2023-09-15 12:20:54.567Z','staff98334',0);
INSERT INTO "class_levels" VALUES (1,'2023-09-16 08:59:46.218Z','','5ozj234dplkmxcj','Class One','2023-09-16 08:59:46.218Z');
INSERT INTO "class_levels" VALUES (0,'2023-09-16 09:05:32.963Z','','h4vghjnolimdo14','PP1','2023-09-16 09:05:32.963Z');
INSERT INTO "subjects" VALUES ('2023-09-16 09:02:27.686Z','Logics and Numbers','vdhch1i5hbbs3tf','Mathematics','2023-09-16 09:02:27.686Z');
INSERT INTO "subjects_per_classes" VALUES ('5ozj234dplkmxcj','2023-09-16 09:02:42.233Z','6lsimrqe00y1z4y','vdhch1i5hbbs3tf','2023-09-16 09:03:54.371Z','Maths class one');
INSERT INTO "classes" VALUES ('2023-08-28 12:00:00.000Z','2023-09-16 09:01:19.239Z','5s7zisb1a3kajo9','5ozj234dplkmxcj','0i8719g7sqks9vn','2023-09-16 09:01:19.239Z','2023-01-02 12:00:00.000Z','{
  "motto":"Long is never forever."
}');
INSERT INTO "classes" VALUES ('2022-09-05 12:00:00.000Z','2023-09-16 09:06:56.833Z','3wr050v8b1k0181','h4vghjnolimdo14','0i8719g7sqks9vn','2023-09-16 09:06:56.833Z','2022-01-03 12:00:00.000Z','{
"motto":"The Best isn`t here yet."  
}');
INSERT INTO "student_class_enrollment" VALUES ('5s7zisb1a3kajo9','2023-09-16 09:02:53.783Z','pwq0euo936neo6e','nithx4ol1hu6y75','6icitq7qbvlqtkr','2023-09-16 09:02:53.783Z','["6lsimrqe00y1z4y"]');
INSERT INTO "student_class_enrollment" VALUES ('3wr050v8b1k0181','2023-09-16 09:07:28.817Z','5a9wuz2pda34aoh','nithx4ol1hu6y75','6icitq7qbvlqtkr','2023-09-16 09:07:28.817Z','["6lsimrqe00y1z4y"]');
CREATE UNIQUE INDEX IF NOT EXISTS "__pb_users_auth__username_idx" ON "students" (
	"username"
);
CREATE UNIQUE INDEX IF NOT EXISTS "__pb_users_auth__email_idx" ON "students" (
	"email"
) WHERE "email" != '';
CREATE UNIQUE INDEX IF NOT EXISTS "__pb_users_auth__tokenKey_idx" ON "students" (
	"tokenKey"
);
CREATE UNIQUE INDEX IF NOT EXISTS "idx_K1ayASw" ON "librarians" (
	"staff_id",
	"library_id"
);
CREATE UNIQUE INDEX IF NOT EXISTS "idx_8nBVg9e" ON "books_collections" (
	"index",
	"book_id"
);
CREATE UNIQUE INDEX IF NOT EXISTS "_bj1d7ix66idox0r_username_idx" ON "staff" (
	"username"
);
CREATE UNIQUE INDEX IF NOT EXISTS "_bj1d7ix66idox0r_email_idx" ON "staff" (
	"email"
) WHERE "email" != '';
CREATE UNIQUE INDEX IF NOT EXISTS "_bj1d7ix66idox0r_tokenKey_idx" ON "staff" (
	"tokenKey"
);
CREATE UNIQUE INDEX IF NOT EXISTS "idx_L1Ib1Jj" ON "accountings" (
	"section"
);
CREATE UNIQUE INDEX IF NOT EXISTS "idx_VmigVtU" ON "accountants" (
	"staff_id",
	"accounting_id"
);
CREATE UNIQUE INDEX IF NOT EXISTS "idx_lLPvcjL" ON "subjects_per_class_staff" (
	"subjects_per_class_id",
	"staff_id",
	"from_date"
);
CREATE UNIQUE INDEX IF NOT EXISTS "idx_U6d1LSI" ON "class_attendances" (
	"class_id",
	"student_id",
	"date"
);
CREATE UNIQUE INDEX IF NOT EXISTS "idx_tfXthyu" ON "test_questions" (
	"test_id",
	"question_id"
);
CREATE UNIQUE INDEX IF NOT EXISTS "idx_IQH33Z5" ON "test_questions_student_answers" (
	"test_question_id",
	"student_id"
);
CREATE VIEW `view_students` AS SELECT * FROM (SELECT 
  s.id,
  s.name,
  s.username,
  s.avatar,
  s.email,
  s.verified,
  s.emailVisibility,
  s.created,
  s.updated,
  COUNT(DISTINCT b.id) AS number_of_books_borrowed,
  JSON_ARRAY(
    JSON_OBJECT(
      'class_id', sce.class_id,
      'subjects', JSON_ARRAY(sce.subjects_ids)
    )
  ) AS classes_enrolled_to
FROM students AS s
LEFT JOIN
  books_collections_borrowed AS b ON s.id = b.student_id
LEFT JOIN
  student_class_enrollment AS sce ON s.id = sce.student_id
GROUP BY s.id);
COMMIT;
