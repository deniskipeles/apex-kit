use super::get_cli_db;
use apexkit_core::auth::password;
use apexkit_core::security::vault::MasterKey;
use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum UserCmd {
    /// Create a new user (or Admin)
    Create {
        #[arg(long)]
        email: String,
        #[arg(long)]
        password: Option<String>,
        /// Role: 'admin' or 'user'
        #[arg(long, default_value = "user")]
        role: String,
    },
    /// Reset a user's password manually
    ResetPassword {
        email: String,
        new_password: Option<String>,
    },
    /// List all users
    List,
}

pub async fn execute(cmd: UserCmd) -> Result<(), String> {
    let db = get_cli_db().await?;

    match cmd {
        UserCmd::Create {
            email,
            password,
            role,
        } => {
            if !email.contains('@') {
                return Err("Invalid email format.".into());
            }

            if db
                .get_user_by_email(&email)
                .await
                .map_err(|e| e.to_string())?
                .is_some()
            {
                return Err(format!("User '{}' already exists.", email));
            }

            let raw_password = password.unwrap_or_else(|| {
                let p = MasterKey::generate_random_password();
                println!("⚠️  No password provided. Generated: {}", p);
                p
            });

            let hash = password::hash_password(&raw_password).map_err(|e| e.to_string())?;
            let user = db
                .create_user(&email, &hash, &role, None)
                .await
                .map_err(|e| e.to_string())?;

            println!("✅ User created: {} (ID: {})", user.email, user.id);
            Ok(())
        }
        UserCmd::ResetPassword {
            email,
            new_password,
        } => {
            let user = db
                .get_user_by_email(&email)
                .await
                .map_err(|e| e.to_string())?
                .ok_or(format!("User '{}' not found", email))?;

            let raw_password = new_password.unwrap_or_else(|| {
                let p = MasterKey::generate_random_password();
                println!("⚠️  Generated new password: {}", p);
                p
            });

            let hash = password::hash_password(&raw_password).map_err(|e| e.to_string())?;

            db.update_user(user.id, None, None, None, Some(hash))
                .await
                .map_err(|e| e.to_string())?;

            println!("✅ Password reset successfully for {}", email);
            Ok(())
        }
        UserCmd::List => {
            let users = db
                .list_users(None, 1000, 0)
                .await
                .map_err(|e| e.to_string())?;

            println!("{:<5} {:<30} {:<10}", "ID", "EMAIL", "ROLE");
            println!("{:-<50}", "");
            for u in users {
                println!("{:<5} {:<30} {:<10}", u.id, u.email, u.role);
            }
            Ok(())
        }
    }
}
