# Example: Learning Management System (LMS)

A platform to host courses, track student progress, and issue certificates.

## 1. Database Collections

### `courses`
| Field | Type | Constraints | Options |
| :--- | :--- | :--- | :--- |
| `title` | Text | Required | |
| `description` | Rich Text | | |
| `instructor_id` | Relation | Required | Collection: `users` |

### `lessons`
| Field | Type | Constraints | Options |
| :--- | :--- | :--- | :--- |
| `course_id` | Relation | Required | Collection: `courses` |
| `title` | Text | Required | |
| `video_url` | Text | | |
| `order` | Number | | |

### `progress`
| Field | Type | Constraints | Options |
| :--- | :--- | :--- | :--- |
| `user_id` | Relation | Required | Collection: `users` |
| `lesson_id` | Relation | Required | Collection: `lessons` |
| `completed` | Bool | Default: `false` | |

## 2. Security Policies

- **`courses/lessons`**:
  - `read`: `public` (or purchased)
- **`progress`**:
  - `read/update`: `owner:user_id`

## 3. Progress Tracking (SDK)

```javascript
async function completeLesson(lessonId) {
    await apex.collection('progress').create({
        lesson_id: lessonId,
        user_id: currentUser.id,
        completed: true
    });
}
```

## 4. Course Analytics (Edge Function)

### `get-course-stats`
**Trigger**: GraphQL Query
```javascript
export default async function(args) {
    const courseId = args.id;

    const studentCount = await $db.records.count('enrollments', { course_id: courseId });
    const completionRate = await $db.query(`
        SELECT AVG(completed) as rate
        FROM progress p
        JOIN lessons l ON p.lesson_id = l.id
        WHERE l.course_id = ?
    `, [courseId]);

    return {
        students: studentCount,
        rate: completionRate[0].rate
    };
}
```
