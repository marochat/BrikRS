//! ページの公開範囲：誰がどのページを見られるか
//!
//! - 下書き（is_draft）：作った人と管理者だけ
//! - 公開範囲（visibility）：public（誰でも）/ members（ログインしている人）/ groups（page_groups のグループの人）
//! - 管理者は全部見られる（「管理者」という特別なグループの扱い）
//! - 祖先のどれか1つでも見られなければ、そのページも見られない（下書きと同じく子孫に効く）

use std::collections::{HashMap, HashSet};

use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter, QuerySelect};

use crate::{
    api::pages::PageSummary,
    auth::CurrentUser,
    entity::{page_group, user_group, user_group_member},
    error::{AppError, AppResult},
};

pub const PUBLIC: &str = "public";
pub const MEMBERS: &str = "members";
pub const GROUPS: &str = "groups";
pub const VISIBILITIES: &[&str] = &[PUBLIC, MEMBERS, GROUPS];

/// ページを見ようとしている人
#[derive(Clone, Debug)]
pub enum Viewer {
    /// ログインしていない人
    Guest,
    User {
        id: i32,
        is_admin: bool,
        /// 所属グループ（管理者は使わないので読み込まない）
        groups: HashSet<i32>,
    },
}

impl Viewer {
    /// サーバー上の CLI（管理者と同じく全部見える）
    pub fn cli(user_id: i32) -> Self {
        Viewer::User { id: user_id, is_admin: true, groups: HashSet::new() }
    }

    pub fn is_admin(&self) -> bool {
        matches!(self, Viewer::User { is_admin: true, .. })
    }

    pub fn is_guest(&self) -> bool {
        matches!(self, Viewer::Guest)
    }

    pub fn user_id(&self) -> Option<i32> {
        match self {
            Viewer::User { id, .. } => Some(*id),
            Viewer::Guest => None,
        }
    }

    /// 公開範囲・下書きを切り替えられるか（作った人と管理者）
    pub fn can_manage(&self, created_by: Option<i32>) -> bool {
        self.is_admin() || (created_by.is_some() && created_by == self.user_id())
    }

    /// このページ自身の設定で見られるか（祖先は見ない）
    pub fn sees_own(&self, p: &PageSummary, page_groups: &HashMap<i32, HashSet<i32>>) -> bool {
        match self {
            Viewer::Guest => !p.is_draft && p.visibility == PUBLIC,
            Viewer::User { is_admin: true, .. } => true,
            Viewer::User { id, groups, .. } => {
                if p.is_draft {
                    return p.created_by == Some(*id);
                }
                match p.visibility.as_str() {
                    PUBLIC | MEMBERS => true,
                    GROUPS => page_groups
                        .get(&p.id)
                        .is_some_and(|g| !g.is_disjoint(groups)),
                    // 知らない値なら見せない
                    _ => false,
                }
            }
        }
    }
}

/// ログインしている人の所属グループを読み込んで Viewer にする
pub async fn viewer(db: &impl ConnectionTrait, cu: Option<&CurrentUser>) -> AppResult<Viewer> {
    let Some(cu) = cu else { return Ok(Viewer::Guest) };
    let groups = if cu.user.is_admin {
        HashSet::new()
    } else {
        user_groups_of(db, cu.user.id).await?
    };
    Ok(Viewer::User { id: cu.user.id, is_admin: cu.user.is_admin, groups })
}

pub async fn user_groups_of(db: &impl ConnectionTrait, user_id: i32) -> AppResult<HashSet<i32>> {
    let ids: Vec<i32> = user_group_member::Entity::find()
        .select_only()
        .column(user_group_member::Column::GroupId)
        .filter(user_group_member::Column::UserId.eq(user_id))
        .into_tuple()
        .all(db)
        .await?;
    Ok(ids.into_iter().collect())
}

/// ページ id → 見られるグループ（公開範囲が groups のページの分）
pub async fn page_groups(db: &impl ConnectionTrait) -> AppResult<HashMap<i32, HashSet<i32>>> {
    let rows: Vec<(i32, i32)> = page_group::Entity::find()
        .select_only()
        .columns([page_group::Column::PageId, page_group::Column::GroupId])
        .into_tuple()
        .all(db)
        .await?;
    let mut out: HashMap<i32, HashSet<i32>> = HashMap::new();
    for (p, g) in rows {
        out.entry(p).or_default().insert(g);
    }
    Ok(out)
}

/// 1つのページの見られるグループ（id 順）
pub async fn groups_of_page(db: &impl ConnectionTrait, page_id: i32) -> AppResult<Vec<i32>> {
    let mut ids: Vec<i32> = page_group::Entity::find()
        .select_only()
        .column(page_group::Column::GroupId)
        .filter(page_group::Column::PageId.eq(page_id))
        .into_tuple()
        .all(db)
        .await?;
    ids.sort_unstable();
    Ok(ids)
}

/// viewer に見えないページの id（自分の設定で見えないページと、その子孫）
pub fn hidden_ids(
    all: &[PageSummary],
    page_groups: &HashMap<i32, HashSet<i32>>,
    viewer: &Viewer,
) -> HashSet<i32> {
    if viewer.is_admin() {
        return HashSet::new();
    }
    let mut kids: HashMap<i32, Vec<i32>> = HashMap::new();
    for p in all {
        if let Some(pid) = p.parent_id {
            kids.entry(pid).or_default().push(p.id);
        }
    }
    let mut hidden = HashSet::new();
    let mut stack: Vec<i32> = all
        .iter()
        .filter(|p| !viewer.sees_own(p, page_groups))
        .map(|p| p.id)
        .collect();
    while let Some(id) = stack.pop() {
        if hidden.insert(id) {
            stack.extend(kids.get(&id).into_iter().flatten());
        }
    }
    hidden
}

/// start と、その祖先のうち、下書きか公開以外の範囲のページの id。
/// ページを start の下に置いたとき、そのページに効く祖先の制限（ページを別の親へ移すと、これが変わる）
pub fn restricting_ancestors(all: &[PageSummary], start: Option<i32>) -> HashSet<i32> {
    let by_id: HashMap<i32, &PageSummary> = all.iter().map(|p| (p.id, p)).collect();
    let mut out = HashSet::new();
    let mut cur = start;
    let mut steps = 0;
    while let Some(c) = cur {
        let Some(p) = by_id.get(&c) else { break };
        if p.is_draft || p.visibility != PUBLIC {
            out.insert(c);
        }
        steps += 1;
        if steps > all.len() {
            // 循環（move_to で防いでいるが念のため）。どこへ移しても「変わる」扱いになるようにしておく
            out.insert(i32::MIN);
            break;
        }
        cur = p.parent_id;
    }
    out
}

/// 祖先（自分は含まない）のどれかに、下書きか公開以外の範囲があるか
pub fn restricted_by_ancestors(all: &[PageSummary], id: i32) -> bool {
    let parent = all.iter().find(|p| p.id == id).and_then(|p| p.parent_id);
    !restricting_ancestors(all, parent).is_empty()
}

/// 保存しようとしている公開範囲を確かめ、グループの id を重複なしで返す。
/// 自分が入っていないグループだけにすると自分も見られなくなるので断る（管理者は全部見えるので例外）
pub async fn validate(
    db: &impl ConnectionTrait,
    viewer: &Viewer,
    visibility: &str,
    group_ids: &[i32],
) -> AppResult<Vec<i32>> {
    if !VISIBILITIES.contains(&visibility) {
        return Err(AppError::BadRequest("不明な公開範囲です".into()));
    }
    if visibility != GROUPS {
        return Ok(vec![]);
    }
    let mut ids: Vec<i32> = group_ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    if !ids.is_empty() {
        let found = user_group::Entity::find()
            .filter(user_group::Column::Id.is_in(ids.clone()))
            .count(db)
            .await?;
        if found != ids.len() as u64 {
            return Err(AppError::BadRequest("存在しないグループが含まれています".into()));
        }
    }
    if let Viewer::User { is_admin: false, groups, .. } = viewer {
        if !ids.iter().any(|g| groups.contains(g)) {
            return Err(AppError::BadRequest(
                "自分が入っているグループを1つ以上選んでください（自分も見られなくなるため）".into(),
            ));
        }
    }
    Ok(ids)
}

/// ページの見られるグループを置き換える
pub async fn set_page_groups(
    db: &impl ConnectionTrait,
    page_id: i32,
    group_ids: &[i32],
) -> AppResult<()> {
    page_group::Entity::delete_many()
        .filter(page_group::Column::PageId.eq(page_id))
        .exec(db)
        .await?;
    if !group_ids.is_empty() {
        let rows: Vec<page_group::ActiveModel> = group_ids
            .iter()
            .map(|&g| page_group::ActiveModel {
                page_id: sea_orm::Set(page_id),
                group_id: sea_orm::Set(g),
            })
            .collect();
        page_group::Entity::insert_many(rows).exec(db).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(id: i32, parent_id: Option<i32>, visibility: &str, is_draft: bool, created_by: i32) -> PageSummary {
        PageSummary {
            id,
            parent_id,
            title: String::new(),
            icon: String::new(),
            sort_order: 0,
            is_draft,
            visibility: visibility.into(),
            created_by: Some(created_by),
            layout: "page".into(),
            is_template: false,
            is_locked: false,
        }
    }

    fn user(id: i32, groups: &[i32]) -> Viewer {
        Viewer::User { id, is_admin: false, groups: groups.iter().copied().collect() }
    }

    /// 1 公開 → 2 登録ユーザー → 3 公開（孫）
    /// 4 グループ10 → 5 グループ20（子）
    /// 6 下書き（ユーザー1）→ 7 公開（子・ユーザー2）→ 8 公開（孫）
    /// 9 グループ（グループ無し＝管理者だけ）
    fn tree() -> (Vec<PageSummary>, HashMap<i32, HashSet<i32>>) {
        let all = vec![
            page(1, None, PUBLIC, false, 1),
            page(2, Some(1), MEMBERS, false, 1),
            page(3, Some(2), PUBLIC, false, 1),
            page(4, None, GROUPS, false, 1),
            page(5, Some(4), GROUPS, false, 1),
            page(6, None, PUBLIC, true, 1),
            page(7, Some(6), PUBLIC, false, 2),
            page(8, Some(7), PUBLIC, false, 2),
            page(9, None, GROUPS, false, 1),
        ];
        let pg = HashMap::from([(4, HashSet::from([10])), (5, HashSet::from([20]))]);
        (all, pg)
    }

    #[test]
    fn guest_sees_only_public_without_restricted_ancestors() {
        let (all, pg) = tree();
        assert_eq!(hidden_ids(&all, &pg, &Viewer::Guest), HashSet::from([2, 3, 4, 5, 6, 7, 8, 9]));
    }

    #[test]
    fn members_and_groups_are_inherited() {
        let (all, pg) = tree();
        // グループ無し：グループのページは見えない。下書きは作った人だけ（子孫も）
        assert_eq!(hidden_ids(&all, &pg, &user(2, &[])), HashSet::from([4, 5, 6, 7, 8, 9]));
        // グループ10だけ：4 は見えるが、5（グループ20）は見えない
        assert_eq!(hidden_ids(&all, &pg, &user(2, &[10])), HashSet::from([5, 6, 7, 8, 9]));
        // グループ20だけ：親の 4 が見えないので 5 も見えない
        assert_eq!(hidden_ids(&all, &pg, &user(2, &[20])), HashSet::from([4, 5, 6, 7, 8, 9]));
        // 両方：4・5 とも見える
        assert_eq!(hidden_ids(&all, &pg, &user(2, &[10, 20])), HashSet::from([6, 7, 8, 9]));
    }

    #[test]
    fn draft_is_for_its_creator_and_admins() {
        let (all, pg) = tree();
        // 下書きを作った人には、その子孫（他の人が作ったものも）見える
        assert_eq!(hidden_ids(&all, &pg, &user(1, &[10, 20])), HashSet::from([9]));
        let admin = Viewer::User { id: 99, is_admin: true, groups: HashSet::new() };
        assert!(hidden_ids(&all, &pg, &admin).is_empty());
    }

    #[test]
    fn can_manage_only_creator_or_admin() {
        assert!(user(1, &[]).can_manage(Some(1)));
        assert!(!user(2, &[]).can_manage(Some(1)));
        assert!(!user(2, &[]).can_manage(None));
        assert!(Viewer::cli(5).can_manage(None));
        assert!(!Viewer::Guest.can_manage(None));
    }

    #[test]
    fn restricted_ancestors() {
        let (all, _) = tree();
        assert!(!restricted_by_ancestors(&all, 1));
        assert!(!restricted_by_ancestors(&all, 2)); // 親の 1 は公開
        assert!(restricted_by_ancestors(&all, 3)); // 親の 2 が登録ユーザー
        assert!(restricted_by_ancestors(&all, 5));
        assert!(restricted_by_ancestors(&all, 8)); // 祖父の 6 が下書き
        assert!(!restricted_by_ancestors(&all, 6));
    }

    #[test]
    fn restricting_ancestors_of_a_place() {
        let (all, _) = tree();
        assert!(restricting_ancestors(&all, None).is_empty());
        assert!(restricting_ancestors(&all, Some(1)).is_empty());
        assert_eq!(restricting_ancestors(&all, Some(3)), HashSet::from([2]));
        assert_eq!(restricting_ancestors(&all, Some(5)), HashSet::from([4, 5]));
        assert_eq!(restricting_ancestors(&all, Some(8)), HashSet::from([6]));
        // 5 の下 → 4 の下は、制限が減る（5 が外れる）
        assert_ne!(restricting_ancestors(&all, Some(5)), restricting_ancestors(&all, Some(4)));
        // 7 の下 → 8 の下は、どちらも 6 の下書きだけなので変わらない
        assert_eq!(restricting_ancestors(&all, Some(7)), restricting_ancestors(&all, Some(8)));
    }
}
